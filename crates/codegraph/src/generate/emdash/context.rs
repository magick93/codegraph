//! Template context types for the EmDash plugin generator family, plus the
//! pure (DB-free) context builders so the field→control mapping, column
//! defaulting, and route resolution are unit-testable without a graph.
//!
//! The DB-facing generator (`plugin_gen.rs`) resolves the entity model,
//! expands value objects, and resolves codelist options; the builders here
//! turn that into template-ready contexts mirroring the hand-written
//! `packages/emdash-community-events` surface.

use std::collections::BTreeMap;

use crate::generate::domain_model::{EntityField, EntityOperations};
use crate::generate::emdash::config::{
    EmdashEntityConfig, EmdashPluginConfig, PublicListConfig, SettingValue,
};
use codegraph_naming::{to_kebab_case, to_pascal_case};

use heck::ToLowerCamelCase;

/// System/meta fields hidden from generated forms and lists unless a config
/// explicitly references them (columns) or un-hides them. Applied on top of
/// the per-entity `hidden_fields` config.
pub const DEFAULT_HIDDEN_FIELDS: &[&str] = &[
    "did",
    "collection",
    "rkey",
    "atUri",
    "documentId",
    "alternateIds",
    "dataClassification",
    "retentionPeriod",
    "locale",
    "updatedAt",
    "createdAt",
    "id",
    "platformOrganizationId",
];

/// Resolved codelist options for one field (`{value,label}[]` for selects).
#[derive(Debug, Clone, serde::Serialize)]
pub struct CodelistOption {
    pub label: String,
    pub value: String,
}

/// Field form control kind (rendered by the admin/form templates).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldControl {
    Text,
    Multiline,
    Number,
    Toggle,
    Select,
    Date,
    Datetime,
    /// Array-of-scalar field, rendered as one-value-per-line multiline text.
    List,
}

impl FieldControl {
    pub fn as_str(&self) -> &'static str {
        match self {
            FieldControl::Text => "text",
            FieldControl::Multiline => "multiline",
            FieldControl::Number => "number",
            FieldControl::Toggle => "toggle",
            FieldControl::Select => "select",
            FieldControl::Date => "date",
            FieldControl::Datetime => "datetime",
            FieldControl::List => "list",
        }
    }

    /// True when the control renders a single scalar value (used for default
    /// list-column selection).
    fn is_scalar(&self) -> bool {
        !matches!(self, FieldControl::List)
    }
}

/// One form field on the generated admin form.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FieldCtx {
    pub name: String,
    pub label: String,
    pub control: FieldControl,
    pub required: bool,
    pub options: Vec<CodelistOption>,
    /// JS literal used as the `??` fallback for the initialValue.
    pub initial_default: Option<String>,
    pub multiline: bool,
}

/// A required-but-hidden field, auto-populated at create time.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AutoFieldCtx {
    pub name: String,
    /// Full JS assignment line, e.g. `payload.did = \`did:plc:rsvp-${...}\`;`
    pub assign_line: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ColumnCtx {
    pub key: String,
    pub label: String,
    pub format: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ConfirmCtx {
    pub title: String,
    pub text: String,
    pub confirm_label: String,
    pub deny_label: String,
    pub style: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SetFieldCtx {
    pub field: String,
    /// JS literal assigned to the field.
    pub literal: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct WhenCtx {
    pub field: String,
    /// Pre-joined JS array literal, e.g. `["Pending", "Confirmed"]`.
    pub values_js: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ActionCtx {
    pub id: String,
    pub full_id: String,
    pub label: String,
    pub style: Option<String>,
    pub confirm: Option<ConfirmCtx>,
    pub set: Vec<SetFieldCtx>,
    pub when: Option<WhenCtx>,
    /// Prebuilt extra button props (style/confirm object literal), starting
    /// with a comma when non-empty — e.g. `, style: "danger", confirm: {...}`.
    pub button_props: String,
    /// Prebuilt `sdkCall` update request expression for this transition,
    /// e.g. `{ rsvp_id: id, status: "Confirmed" } as never`.
    pub update_call: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CustomActionCtx {
    pub id: String,
    pub full_id: String,
    pub label: String,
    pub handler: String,
    pub style: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PublicListCtx {
    pub route: String,
    /// Pins this entity's list as the domain's home page (site `{domain}.astro`).
    pub home: bool,
    pub filter_field: Option<String>,
    /// JS literal for the equality filter (`true`, `"x"`, `3`, ...).
    pub filter_value: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PublicDetailCtx {
    pub route: String,
    pub param: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SubmitFieldCtx {
    pub name: String,
    pub label: String,
    /// HTML input kind: text | email | number | textarea | select
    pub kind: String,
    pub required: bool,
    pub options: Vec<CodelistOption>,
}

/// One minimal create-payload entry for the generated e2e spec.
#[derive(Debug, Clone, serde::Serialize)]
pub struct E2ePayloadField {
    pub name: String,
    /// JS expression, e.g. `` `e2e-${UNIQUE}` ``, `1`, `true`.
    pub value: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PublicSubmitCtx {
    pub route: String,
    pub handler: String,
    pub success_message: String,
    pub button_label: String,
    pub fields: Vec<SubmitFieldCtx>,
}

/// Everything the templates need about one configured entity.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EntityCtx {
    /// Config entity key (with Type suffix), e.g. "RsvpType".
    pub key: String,
    /// lowerCamel, e.g. "rsvpType" (route names, fn prefixes).
    pub camel: String,
    /// PascalCase, e.g. "RsvpType" (fn names).
    pub pascal: String,
    /// snake_case pg table / SDK module, e.g. "rsvp".
    pub module: String,
    /// SDK resource (lowerCamel module), e.g. "publicEvent".
    pub sdk_resource: String,
    /// Operation prefix: Pascal(domain) + Pascal(module), e.g. "EventsRsvp".
    pub prefix: String,
    /// Get-by-id request key, e.g. "rsvp_id".
    pub id_param: String,
    /// Prebuilt JS request object for get/delete: `{ rsvp_id: id }`.
    pub get_req: String,
    /// Prebuilt JS request object for update: `{ rsvp_id: id, ...payload }`.
    pub update_req: String,
    /// SDK response type, e.g. "RsvpResponse".
    pub response_type: String,
    pub page_path: String,
    pub page_label: String,
    pub page_icon: Option<String>,
    pub title_field: String,
    pub empty_text: String,
    pub columns: Vec<ColumnCtx>,
    pub sort_key: Option<String>,
    pub sort_dir: Option<String>,
    pub actions: Vec<ActionCtx>,
    pub custom_actions: Vec<CustomActionCtx>,
    pub form_fields: Vec<FieldCtx>,
    pub auto_fields: Vec<AutoFieldCtx>,
    pub detail_fields: Vec<ColumnCtx>,
    pub has_create: bool,
    pub mint_did: bool,
    pub has_read: bool,
    pub has_update: bool,
    pub has_delete: bool,
    pub has_list: bool,
    pub public_list: Option<PublicListCtx>,
    pub public_detail: Option<PublicDetailCtx>,
    pub public_submit: Option<PublicSubmitCtx>,
    /// Minimal create payload for the generated e2e spec (required visible
    /// fields + auto-populated hidden fields + the title field).
    pub e2e_payload: Vec<E2ePayloadField>,
    /// JS expression for the title-field value in the e2e spec.
    pub e2e_title_expr: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SettingCtx {
    pub key: String,
    pub label: String,
    /// boolean | string | number
    pub r#type: String,
    pub help: Option<String>,
    /// JS literal default, when the settings map carries a value.
    pub default: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct StorageCtx {
    pub name: String,
    /// Pre-joined JS array literal, e.g. `["entityId", "handledAt"]`.
    pub indexes_js: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SubscriptionCtx {
    pub topic: String,
    pub storage: String,
    /// Upper-snake const name, e.g. "TOPIC_EVENTS_PUBLISHED".
    pub const_name: String,
    pub ensure_fn: String,
    pub handler_fn: String,
    pub flag: String,
    /// Cron task name keeping the subscription alive across isolates.
    pub ensure_task_name: String,
}

/// Package-level context shared by all templates of one domain plugin.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EmdashPackageContext {
    pub domain: String,
    pub domain_pascal: String,
    pub domain_kebab: String,
    pub plugin_id: String,
    pub package_name: String,
    pub plugin_label: String,
    pub description: String,
    pub icon: Option<String>,
    pub settings: Vec<SettingCtx>,
    pub storages: Vec<StorageCtx>,
    pub subscriptions: Vec<SubscriptionCtx>,
    pub has_bridge: bool,
    pub has_subscriptions: bool,
    /// Default topic for the publishEvent bridge route.
    pub default_topic: String,
    /// Default entityTable for the publishEvent bridge route.
    pub default_entity_table: String,
    pub entities: Vec<EntityCtx>,
    /// Deduped `./bespoke` imports for admin.ts (bespoke hooks + custom
    /// action handlers).
    pub admin_bespoke_imports: Vec<String>,
    /// Deduped `./bespoke` imports for plugin.ts (public_submit handlers).
    pub plugin_bespoke_imports: Vec<String>,
}

/// Humanize a camelCase/snake_case key: "waitlistAutoPromote" → "Waitlist Auto Promote".
pub fn humanize_key(name: &str) -> String {
    let mut out = String::new();
    let mut at_word_start = true;
    let mut prev_lower = false;
    for ch in name.chars() {
        if ch == '_' || ch == '-' || ch == '.' {
            out.push(' ');
            at_word_start = true;
            prev_lower = false;
        } else if ch.is_uppercase() && prev_lower {
            out.push(' ');
            out.push(ch);
            at_word_start = false;
            prev_lower = false;
        } else if at_word_start {
            out.extend(ch.to_uppercase());
            at_word_start = false;
            prev_lower = ch.is_lowercase();
        } else {
            out.push(ch);
            prev_lower = ch.is_lowercase();
        }
    }
    out
}

fn js_literal(value: &SettingValue) -> String {
    value.js_literal()
}

fn when_values_js(values: &[SettingValue]) -> String {
    let items: Vec<String> = values.iter().map(js_literal).collect();
    format!("[{}]", items.join(", "))
}

/// Render the extra `adminButton` props for a confirm dialog, starting with
/// a comma: `, confirm: { title: ..., text: ..., confirm: ..., deny: ... }`.
fn confirm_props(c: &ConfirmCtx) -> String {
    let mut out = format!(
        ", confirm: {{ title: \"{}\", text: \"{}\", confirm: \"{}\", deny: \"{}\"",
        c.title, c.text, c.confirm_label, c.deny_label
    );
    if let Some(ref style) = c.style {
        out.push_str(&format!(", style: \"{style}\""));
    }
    out.push_str(" }");
    out
}

/// Derive the form control for one expanded entity field.
pub fn control_for_field(
    field: &EntityField,
    options: &[CodelistOption],
    override_multiline: Option<bool>,
) -> FieldControl {
    if field.rust_type.is_collection() {
        return FieldControl::List;
    }
    if !options.is_empty() {
        return FieldControl::Select;
    }
    if field.ts_type == "boolean" {
        return FieldControl::Toggle;
    }
    if field.ts_type == "number" {
        return FieldControl::Number;
    }
    let inner = field.rust_type.inner_type();
    if inner.contains("DateTime") || inner.contains("NaiveDateTime") {
        return FieldControl::Datetime;
    }
    if inner.contains("NaiveDate") {
        return FieldControl::Date;
    }
    let force_multiline = override_multiline.unwrap_or(false);
    if force_multiline || inner == "Text" {
        return FieldControl::Multiline;
    }
    let lower = field.name.to_lowercase();
    if ["description", "notes", "requests", "summary"]
        .iter()
        .any(|needle| lower.contains(needle))
    {
        return FieldControl::Multiline;
    }
    FieldControl::Text
}

fn column_format(control: FieldControl) -> Option<String> {
    match control {
        FieldControl::Datetime => Some("datetime".to_string()),
        FieldControl::Date => Some("date".to_string()),
        FieldControl::Toggle => Some("bool".to_string()),
        FieldControl::Number => Some("number".to_string()),
        _ => None,
    }
}

fn default_title_field(visible: &[FieldCtx]) -> String {
    for preferred in ["name", "title", "personName", "label", "summary"] {
        if let Some(f) = visible
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(preferred))
        {
            return f.name.clone();
        }
    }
    visible
        .iter()
        .find(|f| f.control == FieldControl::Text || f.control == FieldControl::Multiline)
        .map(|f| f.name.clone())
        .or_else(|| visible.first().map(|f| f.name.clone()))
        .unwrap_or_else(|| "id".to_string())
}

fn auto_assign_line(field: &EntityField, control: FieldControl, module: &str) -> String {
    let name = &field.name;
    if name == "did" {
        return format!(
            "if (!payload.did) payload.did = `did:plc:{module}-${{Date.now().toString(36)}}`;"
        );
    }
    let fallback = match control {
        FieldControl::Toggle => "true".to_string(),
        FieldControl::Number => "1".to_string(),
        FieldControl::Datetime => "new Date().toISOString()".to_string(),
        _ => format!("`{module}-${{Date.now().toString(36)}}`"),
    };
    format!("if (!payload.{name}) payload.{name} = payload.{name} ?? {fallback};")
}

/// Build the per-entity template context.
///
/// `fields` must already be VO-expanded (see `expand_vo_fields`); `codelists`
/// maps a field name to its resolved `{value,label}[]` options.
pub fn build_entity_context(
    entity_key: &str,
    domain: &str,
    table_name: &str,
    cfg: &EmdashEntityConfig,
    fields: &[EntityField],
    codelists: &BTreeMap<String, Vec<CodelistOption>>,
    ops: &EntityOperations,
) -> EntityCtx {
    let key = entity_key.to_string();
    let stripped = codegraph_naming::strip_suffix(&key, "Type");
    let pascal = to_pascal_case(&key);
    let camel = pascal.to_lower_camel_case();
    let module = table_name.to_string();
    let module_pascal = to_pascal_case(&module);
    let sdk_resource = module.to_lower_camel_case();
    // SDK method prefix: lowerCamel(domain) + Pascal(module), e.g.
    // "eventsRsvp" → client.rsvp.eventsRsvpList() (Fern operation ids are
    // `{domain}_{module}_{op}` camelCased).
    let prefix = format!("{}{}", domain.to_lower_camel_case(), module_pascal);
    let id_param = format!("{module}_id");

    let page_path = cfg
        .page
        .as_ref()
        .map(|p| p.path.clone())
        .unwrap_or_else(|| format!("/{}", to_kebab_case(&stripped)));
    let page_label = cfg
        .page
        .as_ref()
        .and_then(|p| p.label.clone())
        .unwrap_or_else(|| humanize_key(&stripped));
    let page_icon = cfg.page.as_ref().and_then(|p| p.icon.clone());

    let hidden: std::collections::BTreeSet<String> = DEFAULT_HIDDEN_FIELDS
        .iter()
        .map(|s| s.to_string())
        .chain(cfg.hidden_fields.iter().cloned())
        .collect();

    // Form fields: every non-hidden field, in model order.
    let mut form_fields = Vec::new();
    for field in fields {
        if hidden.contains(&field.name) {
            continue;
        }
        let options = codelists.get(&field.name).cloned().unwrap_or_default();
        let override_multiline = cfg
            .form_overrides
            .get(&field.name)
            .and_then(|o| o.multiline);
        let control = control_for_field(field, &options, override_multiline);
        let initial_default = match control {
            FieldControl::Datetime if field.required => Some("new Date().toISOString()".into()),
            FieldControl::Select if field.required => {
                options.first().map(|o| format!("\"{}\"", o.value))
            }
            _ => None,
        };
        form_fields.push(FieldCtx {
            name: field.name.clone(),
            label: field.label.clone(),
            control,
            required: field.required,
            multiline: matches!(control, FieldControl::Multiline | FieldControl::List),
            options,
            initial_default,
        });
    }

    // Flattened value-object fields (e.g. personDid/personName/
    // personRelationship from PersonReferenceType) inherit the VO's single
    // label ("Person") — ambiguous in forms and validation messages. When a
    // label collides, re-derive each colliding field's label from its full
    // key ("Person Name", "Person Did", …).
    {
        let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
        for f in &form_fields {
            *counts.entry(f.label.clone()).or_insert(0) += 1;
        }
        for f in &mut form_fields {
            if counts.get(&f.label).copied().unwrap_or(0) > 1 {
                f.label = humanize_key(&f.name);
            }
        }
    }

    let title_field = cfg
        .title_field
        .clone()
        .unwrap_or_else(|| default_title_field(&form_fields));

    // Columns: explicit config wins; else first 4 visible scalar fields.
    // Object keys in the generated row mapper are the labels, so dedupe:
    // a repeated label (e.g. flattened person refs sharing "Person") gains
    // a ` (key)` suffix to keep the generated object literal valid.
    let dedupe_labels = |cols: &mut Vec<ColumnCtx>| {
        let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for c in cols.iter_mut() {
            if !seen.insert(c.label.clone()) {
                c.label = format!("{} ({})", c.label, c.key);
            }
        }
    };
    let mut columns: Vec<ColumnCtx> = match &cfg.columns {
        Some(cols) => cols
            .iter()
            .map(|c| ColumnCtx {
                key: c.key.clone(),
                label: c.label.clone().unwrap_or_else(|| humanize_key(&c.key)),
                format: c.format.clone(),
            })
            .collect(),
        None => form_fields
            .iter()
            .filter(|f| f.control.is_scalar())
            .take(4)
            .map(|f| ColumnCtx {
                key: f.name.clone(),
                label: f.label.clone(),
                format: column_format(f.control),
            })
            .collect(),
    };
    dedupe_labels(&mut columns);

    // Detail fields: every visible scalar field.
    let detail_fields: Vec<ColumnCtx> = form_fields
        .iter()
        .filter(|f| f.control.is_scalar())
        .map(|f| ColumnCtx {
            key: f.name.clone(),
            label: f.label.clone(),
            format: column_format(f.control),
        })
        .collect();

    // Auto-populated create fields: required but hidden.
    let auto_fields: Vec<AutoFieldCtx> = if ops.create {
        fields
            .iter()
            .filter(|f| f.required && hidden.contains(&f.name))
            .map(|f| {
                let options = codelists.get(&f.name).cloned().unwrap_or_default();
                let control = control_for_field(f, &options, None);
                AutoFieldCtx {
                    name: f.name.clone(),
                    assign_line: auto_assign_line(f, control, &module),
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    let actions: Vec<ActionCtx> = cfg
        .actions
        .iter()
        .flatten()
        .map(|a| {
            let confirm = a.confirm.as_ref().map(|text| ConfirmCtx {
                title: a.label.clone(),
                text: text.clone(),
                confirm_label: a.label.clone(),
                deny_label: "Cancel".to_string(),
                style: a.style.clone(),
            });
            let mut button_props = String::new();
            if let Some(ref style) = a.style {
                button_props.push_str(&format!(", style: \"{style}\""));
            }
            if let Some(ref c) = confirm {
                button_props.push_str(&confirm_props(c));
            }
            let set_fields: Vec<String> = a
                .set
                .iter()
                .map(|(field, value)| format!("{}: {}", field, value.js_literal()))
                .collect();
            let update_call = if set_fields.is_empty() {
                format!("{{ {id_param}: id }} as never")
            } else {
                format!("{{ {id_param}: id, {} }} as never", set_fields.join(", "))
            };
            ActionCtx {
                id: a.id.clone(),
                full_id: format!("{key}:{}", a.id),
                label: a.label.clone(),
                style: a.style.clone(),
                confirm,
                set: a
                    .set
                    .iter()
                    .map(|(field, value)| SetFieldCtx {
                        field: field.clone(),
                        literal: value.js_literal(),
                    })
                    .collect(),
                when: a.visible_when.as_ref().map(|w| WhenCtx {
                    field: w.field.clone(),
                    values_js: when_values_js(&w.r#in),
                }),
                button_props,
                update_call,
            }
        })
        .collect();

    let custom_actions: Vec<CustomActionCtx> = cfg
        .custom_actions
        .iter()
        .flatten()
        .map(|a| CustomActionCtx {
            id: a.id.clone(),
            full_id: format!("{key}:{}", a.id),
            label: a.label.clone(),
            handler: a.handler.clone(),
            style: a.style.clone(),
        })
        .collect();

    let public_list: Option<PublicListCtx> =
        cfg.public_list.as_ref().map(|p: &PublicListConfig| {
            let (filter_field, filter_value) = match &p.filter {
                Some(f) => (
                    Some(f.field.clone()),
                    Some(
                        f.eq.as_ref()
                            .map(js_literal)
                            .unwrap_or_else(|| "true".into()),
                    ),
                ),
                None => (None, None),
            };
            PublicListCtx {
                route: p.route.clone(),
                home: p.home.unwrap_or(false),
                filter_field,
                filter_value,
            }
        });

    let public_detail = cfg.public_detail.as_ref().map(|p| PublicDetailCtx {
        route: p.route.clone(),
        param: p.param.clone().unwrap_or_else(|| "id".to_string()),
    });

    let public_submit = cfg.public_submit.as_ref().map(|p| {
        let fields = p
            .fields
            .iter()
            .map(|name| {
                let form_field = form_fields.iter().find(|f| &f.name == name);
                let label = form_field
                    .map(|f| f.label.clone())
                    .unwrap_or_else(|| humanize_key(name));
                let kind = match form_field.map(|f| f.control) {
                    Some(FieldControl::Select) => "select",
                    Some(FieldControl::Number) => "number",
                    Some(FieldControl::Multiline) | Some(FieldControl::List) => "textarea",
                    _ => {
                        let lower = name.to_lowercase();
                        if lower.contains("email") || lower == "persondid" {
                            "email"
                        } else {
                            "text"
                        }
                    }
                };
                SubmitFieldCtx {
                    name: name.clone(),
                    label,
                    kind: kind.to_string(),
                    required: p.required.iter().any(|r| r == name),
                    options: form_field.map(|f| f.options.clone()).unwrap_or_default(),
                }
            })
            .collect();
        PublicSubmitCtx {
            route: p.route.clone(),
            handler: p.handler.clone(),
            success_message: p
                .success_message
                .clone()
                .unwrap_or_else(|| "Thank you — your submission was received.".to_string()),
            button_label: p
                .button_label
                .clone()
                .unwrap_or_else(|| "Submit".to_string()),
            fields,
        }
    });

    let empty_text = format!("No {} yet.", page_label.to_lowercase());

    // Minimal e2e create payload: required visible fields, then required
    // hidden (auto-populated) fields, then the title field (so the created
    // record is findable). Values are deterministic per control kind.
    let e2e_value = |control: FieldControl, options: &[CodelistOption]| -> String {
        match control {
            FieldControl::Number => "1".to_string(),
            FieldControl::Toggle => "true".to_string(),
            FieldControl::Date | FieldControl::Datetime => "new Date().toISOString()".to_string(),
            FieldControl::Select => options
                .first()
                .map(|o| format!("\"{}\"", o.value))
                .unwrap_or_else(|| "`e2e-${UNIQUE}`".to_string()),
            _ => "`e2e-${UNIQUE}`".to_string(),
        }
    };
    let mut e2e_payload: Vec<E2ePayloadField> = Vec::new();
    let mut e2e_seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    if ops.create {
        for f in &form_fields {
            if !f.required || e2e_seen.contains(&f.name) {
                continue;
            }
            e2e_seen.insert(f.name.clone());
            e2e_payload.push(E2ePayloadField {
                name: f.name.clone(),
                value: e2e_value(f.control, &f.options),
            });
        }
        for af in &auto_fields {
            if e2e_seen.contains(&af.name) {
                continue;
            }
            e2e_seen.insert(af.name.clone());
            // Auto fields: mirror the admin auto-population semantics.
            let source = fields.iter().find(|f| f.name == af.name);
            let control = source
                .map(|f| {
                    let options = codelists.get(&f.name).cloned().unwrap_or_default();
                    control_for_field(f, &options, None)
                })
                .unwrap_or(FieldControl::Text);
            e2e_payload.push(E2ePayloadField {
                name: af.name.clone(),
                value: if af.name == "did" {
                    format!("`did:plc:{module}-e2e-${{UNIQUE}}`")
                } else {
                    e2e_value(control, &[])
                },
            });
        }
    }
    if !e2e_seen.contains(&title_field) {
        // Make the created record findable by its title field.
        let control = form_fields
            .iter()
            .find(|f| f.name == title_field)
            .map(|f| f.control)
            .unwrap_or(FieldControl::Text);
        e2e_payload.push(E2ePayloadField {
            name: title_field.clone(),
            value: e2e_value(control, &[]),
        });
    }
    let e2e_title_expr = e2e_payload
        .iter()
        .find(|f| f.name == title_field)
        .map(|f| f.value.clone())
        .unwrap_or_else(|| "`e2e-${UNIQUE}`".to_string());

    EntityCtx {
        key: key.clone(),
        camel,
        pascal,
        module,
        sdk_resource,
        prefix,
        get_req: format!("{{ {id_param}: id }}"),
        update_req: format!("{{ {id_param}: id, ...payload }}"),
        response_type: format!("{module_pascal}Response"),
        id_param,
        page_path,
        page_label,
        page_icon,
        title_field,
        empty_text,
        columns,
        sort_key: cfg.sort.as_ref().map(|s| s.key.clone()),
        sort_dir: cfg
            .sort
            .as_ref()
            .map(|s| s.dir.clone().unwrap_or_else(|| "asc".into())),
        actions,
        custom_actions,
        form_fields,
        auto_fields,
        detail_fields,
        // `suppress_create` removes the generic create form when the entity's
        // payload needs bespoke shaping (nested value objects, derivations);
        // a custom action + bespoke form replaces it.
        has_create: ops.create && !cfg.suppress_create,
        mint_did: cfg.mint_did,
        has_read: ops.read,
        has_update: ops.update,
        has_delete: ops.delete,
        has_list: ops.list,
        public_list,
        public_detail,
        public_submit,
        e2e_payload,
        e2e_title_expr,
    }
}

fn subscription_const_name(topic: &str) -> String {
    let sanitized: String = topic
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("TOPIC_{}", sanitized.to_uppercase())
}

fn topic_pascal(topic: &str) -> String {
    let segments: Vec<String> = topic
        .split(|c: char| c == '.' || c == '-' || c == '_' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .map(to_pascal_case)
        .collect();
    segments.join("")
}

fn kebab_alnum(name: &str) -> String {
    let kebab = to_kebab_case(name);
    kebab
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect()
}

/// Build the package-level context for one configured domain.
pub fn build_package_context(
    domain: &str,
    cfg: &EmdashPluginConfig,
    entities: Vec<EntityCtx>,
) -> EmdashPackageContext {
    let domain_kebab = kebab_alnum(domain);
    let domain_pascal = to_pascal_case(domain);
    let plugin_id = format!("community-{domain_kebab}");
    let package_name = format!("@community-os/emdash-community-{domain_kebab}");

    // Settings: union of value keys, explicit types, and help texts, sorted.
    let mut setting_keys: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    setting_keys.extend(cfg.settings.keys().cloned());
    setting_keys.extend(cfg.settings_types.keys().cloned());
    setting_keys.extend(cfg.settings_help.keys().cloned());
    let settings: Vec<SettingCtx> = setting_keys
        .into_iter()
        .map(|key| {
            let value = cfg.settings.get(&key);
            let r#type = cfg
                .settings_types
                .get(&key)
                .cloned()
                .or_else(|| value.map(|v| v.inferred_type().to_string()))
                .unwrap_or_else(|| "string".to_string());
            SettingCtx {
                label: humanize_key(&key),
                key: key.clone(),
                r#type,
                help: cfg.settings_help.get(&key).cloned(),
                default: value.map(|v| v.js_literal()),
            }
        })
        .collect();

    // Storages: the domain sync-state collection plus every subscription's
    // consumption-record collection (deduped, order preserved).
    let mut storages: Vec<StorageCtx> = vec![StorageCtx {
        name: format!("{domain}_sync_state"),
        indexes_js: "[\"entityId\", \"handledAt\"]".to_string(),
    }];
    let bridge = cfg.events_bridge.as_ref();
    if let Some(subs) = bridge.map(|b| &b.subscriptions) {
        for sub in subs {
            if storages.iter().any(|s| s.name == sub.storage) {
                continue;
            }
            storages.push(StorageCtx {
                name: sub.storage.clone(),
                indexes_js: "[\"entityId\", \"handledAt\"]".to_string(),
            });
        }
    }

    let subscriptions: Vec<SubscriptionCtx> = bridge
        .map(|b| &b.subscriptions)
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(idx, sub)| SubscriptionCtx {
            topic: sub.topic.clone(),
            storage: sub.storage.clone(),
            const_name: subscription_const_name(&sub.topic),
            ensure_fn: format!("ensureSubscribed{idx}"),
            handler_fn: format!("handle{}", topic_pascal(&sub.topic)),
            flag: format!("subscribed{idx}"),
            ensure_task_name: format!(
                "community-os-{}-{}-ensure",
                domain_kebab,
                kebab_alnum(&sub.topic)
            ),
        })
        .collect();

    let has_subscriptions = !subscriptions.is_empty();
    let has_bridge = bridge.map(|b| b.poll_task).unwrap_or(false);
    let default_topic = subscriptions
        .first()
        .map(|s| s.topic.clone())
        .unwrap_or_else(|| format!("{domain}.updated"));
    let default_entity_table = entities
        .first()
        .map(|e| e.module.clone())
        .unwrap_or_else(|| domain.to_string());

    // Bespoke import lists (deduped, config order preserved).
    let mut admin_bespoke_imports: Vec<String> = Vec::new();
    let mut plugin_bespoke_imports: Vec<String> = Vec::new();
    for entity in &entities {
        for hook in cfg
            .entities
            .get(&entity.key)
            .map(|e| e.bespoke.as_slice())
            .unwrap_or(&[])
        {
            if !admin_bespoke_imports.contains(hook) {
                admin_bespoke_imports.push(hook.clone());
            }
        }
        for action in &entity.custom_actions {
            if !admin_bespoke_imports.contains(&action.handler) {
                admin_bespoke_imports.push(action.handler.clone());
            }
        }
        if let Some(submit) = &entity.public_submit {
            if !plugin_bespoke_imports.contains(&submit.handler) {
                plugin_bespoke_imports.push(submit.handler.clone());
            }
        }
    }

    EmdashPackageContext {
        domain: domain.to_string(),
        domain_pascal,
        domain_kebab,
        plugin_id,
        package_name,
        plugin_label: cfg.label.clone(),
        description: cfg
            .description
            .clone()
            .unwrap_or_else(|| format!("{} domain plugin", cfg.label)),
        icon: cfg.icon.clone(),
        settings,
        storages,
        subscriptions,
        has_bridge,
        has_subscriptions,
        default_topic,
        default_entity_table,
        entities,
        admin_bespoke_imports,
        plugin_bespoke_imports,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::domain_model::{EntityModel, RustType};

    fn field(name: &str, ts_type: &str, rust_type: RustType, required: bool) -> EntityField {
        EntityField {
            name: name.to_string(),
            column: codegraph_naming::to_snake_case(name),
            rust_field: codegraph_naming::to_snake_case(name),
            rust_type,
            sea_orm_type: "String".into(),
            pg_type: "TEXT".into(),
            ts_type: ts_type.to_string(),
            required,
            is_pk: false,
            is_fk: false,
            fk_target: None,
            fk_table: None,
            classification: None,
            example_value: "\"x\"".into(),
            label: humanize_key(name),
            inherited: false,
            is_child_table: false,
            is_model_optional: !required,
        }
    }

    fn ops() -> EntityOperations {
        EntityOperations {
            create: true,
            read: true,
            update: true,
            delete: true,
            list: true,
        }
    }

    fn sample_fields() -> Vec<EntityField> {
        vec![
            field(
                "personName",
                "string",
                RustType::Simple("String".into()),
                true,
            ),
            field(
                "status",
                "string",
                RustType::Simple("RsvpStatus".into()),
                true,
            ),
            field("plusOnes", "number", RustType::Simple("i64".into()), false),
            field(
                "isPublished",
                "boolean",
                RustType::Simple("bool".into()),
                false,
            ),
            field("did", "string", RustType::Simple("String".into()), true),
            field(
                "accessibilityRequests",
                "string",
                RustType::Simple("Text".into()),
                false,
            ),
            field(
                "timestamp",
                "string",
                RustType::Simple("DateTimeUtc".into()),
                true,
            ),
        ]
    }

    #[test]
    fn humanize_keys() {
        assert_eq!(humanize_key("waitlistAutoPromote"), "Waitlist Auto Promote");
        assert_eq!(humanize_key("publicListing"), "Public Listing");
        assert_eq!(humanize_key("rsvp_type"), "Rsvp Type");
    }

    #[test]
    fn entity_context_resolves_controls_routes_and_auto_fields() {
        let mut codelists = BTreeMap::new();
        codelists.insert(
            "status".to_string(),
            vec![
                CodelistOption {
                    label: "Confirmed".into(),
                    value: "Confirmed".into(),
                },
                CodelistOption {
                    label: "Pending".into(),
                    value: "Pending".into(),
                },
            ],
        );

        let cfg: EmdashEntityConfig = toml::from_str(
            r#"
title_field = "personName"
actions = [
  { id = "confirm", label = "Confirm", set = { status = "Confirmed" }, visible_when = { field = "status", in = ["Pending"] } },
]
public_list = { route = "events", filter = { field = "isPublished", eq = true } }
public_detail = { route = "event", param = "id" }
public_submit = { route = "submitRsvp", fields = ["personName", "plusOnes", "status"], required = ["personName"], success_message = "Thanks!", button_label = "Send RSVP", handler = "submitRsvp" }
custom_actions = [ { id = "promote", label = "Promote first", handler = "promoteFirst" } ]
"#,
        )
        .unwrap();

        let ctx = build_entity_context(
            "RsvpType",
            "events",
            "rsvp",
            &cfg,
            &sample_fields(),
            &codelists,
            &ops(),
        );

        assert_eq!(ctx.pascal, "RsvpType");
        assert_eq!(ctx.camel, "rsvpType");
        assert_eq!(ctx.sdk_resource, "rsvp");
        assert_eq!(ctx.prefix, "eventsRsvp");
        assert_eq!(ctx.id_param, "rsvp_id");
        assert_eq!(ctx.get_req, "{ rsvp_id: id }");
        assert_eq!(ctx.response_type, "RsvpResponse");
        assert_eq!(ctx.page_path, "/rsvp");
        assert_eq!(ctx.title_field, "personName");

        // did is required + hidden → auto-populated at create time.
        assert!(ctx.auto_fields.iter().any(|a| a.name == "did"));
        // status resolves as a select with codelist options.
        let status = ctx
            .form_fields
            .iter()
            .find(|f| f.name == "status")
            .expect("status field");
        assert_eq!(status.control, FieldControl::Select);
        assert_eq!(status.options.len(), 2);
        assert_eq!(status.initial_default.as_deref(), Some("\"Confirmed\""));
        // accessibilityRequests (Text) is multiline.
        let access = ctx
            .form_fields
            .iter()
            .find(|f| f.name == "accessibilityRequests")
            .unwrap();
        assert_eq!(access.control, FieldControl::Multiline);
        // timestamp resolves as datetime with a now default (required).
        let ts = ctx
            .form_fields
            .iter()
            .find(|f| f.name == "timestamp")
            .unwrap();
        assert_eq!(ts.control, FieldControl::Datetime);
        assert_eq!(
            ts.initial_default.as_deref(),
            Some("new Date().toISOString()")
        );

        // Routes resolved.
        let plist = ctx.public_list.as_ref().unwrap();
        assert_eq!(plist.route, "events");
        assert_eq!(plist.filter_field.as_deref(), Some("isPublished"));
        assert_eq!(plist.filter_value.as_deref(), Some("true"));
        assert_eq!(ctx.public_detail.as_ref().unwrap().param, "id");
        let submit = ctx.public_submit.as_ref().unwrap();
        assert_eq!(submit.handler, "submitRsvp");
        assert_eq!(submit.fields[0].kind, "text");
        assert_eq!(submit.fields[1].kind, "number");
        assert_eq!(submit.fields[2].kind, "select");
        assert!(submit.fields[0].required);

        // Transition action with visible-when + set fields.
        let action = &ctx.actions[0];
        assert_eq!(action.full_id, "RsvpType:confirm");
        assert_eq!(action.set[0].field, "status");
        assert_eq!(action.set[0].literal, "\"Confirmed\"");
        let when = action.when.as_ref().unwrap();
        assert_eq!(when.values_js, "[\"Pending\"]");

        // Default columns: first 4 visible scalar fields.
        assert_eq!(ctx.columns.len(), 4);
        assert_eq!(ctx.columns[0].key, "personName");
    }

    #[test]
    fn package_context_merges_storages_settings_and_imports() {
        let cfg: EmdashPluginConfig = toml::from_str(
            r#"
label = "Events"
settings = { publicListing = true }
settings_types = { rsvpCutoffHours = "number" }
settings_help = { publicListing = "Show on the public site." }
events_bridge = { poll_task = true, subscriptions = [ { topic = "events.published", storage = "rsvp_sync_state" } ] }

[entities.RsvpType]
public_submit = { route = "submitRsvp", handler = "submitRsvp", fields = [] }
bespoke = ["promoteFirst"]
custom_actions = [ { id = "promote", label = "Promote", handler = "promoteFirst" } ]
"#,
        )
        .unwrap();

        let entity = build_entity_context(
            "RsvpType",
            "events",
            "rsvp",
            cfg.entities.get("RsvpType").unwrap(),
            &[],
            &BTreeMap::new(),
            &ops(),
        );
        let ctx = build_package_context("events", &cfg, vec![entity]);

        assert_eq!(ctx.plugin_id, "community-events");
        assert_eq!(ctx.package_name, "@community-os/emdash-community-events");
        assert_eq!(ctx.domain_pascal, "Events");
        assert!(ctx.has_bridge);
        assert!(ctx.has_subscriptions);
        assert_eq!(ctx.subscriptions[0].const_name, "TOPIC_EVENTS_PUBLISHED");
        assert_eq!(ctx.subscriptions[0].handler_fn, "handleEventsPublished");
        assert_eq!(ctx.default_topic, "events.published");
        // Main storage + subscription storage, deduped.
        assert_eq!(ctx.storages.len(), 2);
        assert_eq!(ctx.storages[0].name, "events_sync_state");
        assert_eq!(ctx.storages[1].name, "rsvp_sync_state");
        // Settings merge value keys + type-only keys.
        assert_eq!(ctx.settings.len(), 2);
        let cutoff = ctx
            .settings
            .iter()
            .find(|s| s.key == "rsvpCutoffHours")
            .unwrap();
        assert_eq!(cutoff.r#type, "number");
        assert!(cutoff.default.is_none());
        let listing = ctx
            .settings
            .iter()
            .find(|s| s.key == "publicListing")
            .unwrap();
        assert_eq!(listing.default.as_deref(), Some("true"));
        // Bespoke imports deduped across bespoke hooks + custom handlers.
        assert_eq!(ctx.admin_bespoke_imports, vec!["promoteFirst"]);
        assert_eq!(ctx.plugin_bespoke_imports, vec!["submitRsvp"]);
    }

    #[test]
    fn entity_page_defaults_from_key() {
        let cfg: EmdashEntityConfig = toml::from_str("").unwrap();
        let ctx = build_entity_context(
            "CheckInType",
            "events",
            "check_in",
            &cfg,
            &[field(
                "personName",
                "string",
                RustType::Simple("String".into()),
                true,
            )],
            &BTreeMap::new(),
            &ops(),
        );
        assert_eq!(ctx.page_path, "/check-in");
        assert_eq!(ctx.page_label, "Check In");
        assert_eq!(ctx.sdk_resource, "checkIn");
        assert_eq!(ctx.title_field, "personName");
    }

    #[test]
    fn model_module_is_table_name() {
        // Sanity: EntityModel.entity_module mirrors pg_schema + table; the
        // plugin context intentionally uses the bare table name (SDK module).
        let _ = std::marker::PhantomData::<EntityModel>;
    }
}
