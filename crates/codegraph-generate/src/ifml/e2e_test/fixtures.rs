//! Fixture payloads and TS-literal/URL-pattern helpers for the IFML e2e
//! generator (issue #317 module split).

use heck::ToLowerCamelCase;
use rex_ifml::{ComponentSpec, FormSpec};

use super::super::api_paths::{ResolvedApi, resolve_entity_api};
use super::super::context::{IfmlAction, IfmlComponent, IfmlEvent, IfmlViewContainer};
use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::{PropertyNode, resolve_field};
use codegraph_type_contracts::RefClassificationKind;

use crate::ddd::design::DddDesignSurface;
use crate::project_config::DtoKeyCasing;

#[derive(Debug, Clone)]
pub struct Fixture {
    pub base_path: String,
    pub entries: Vec<(String, String)>,
    /// The DTO wire-key casing (#539 follow-through): the create/update
    /// DTOs `deny_unknown_fields`, so the JSON keys must match the serde
    /// contract — the schema property name (`submittedAt`) is NOT the key
    /// under the default snake casing (`submitted_at`).
    pub casing: DtoKeyCasing,
}

impl Fixture {
    pub(crate) fn data_literal(&self) -> String {
        let inner = self
            .entries
            .iter()
            .map(|(k, v)| format!("'{}': {v}", self.wire_key(k)))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{{ {inner} }}")
    }

    /// The DTO serde key for a schema property name: the Rust field name
    /// (`to_snake_case`) verbatim under the snake casing, camelCased under
    /// the camel casing (the `rename_all = "camelCase"` contract).
    fn wire_key(&self, schema_field: &str) -> String {
        let rust_field = codegraph_naming::to_snake_case(schema_field);
        match self.casing {
            DtoKeyCasing::Snake => rust_field,
            DtoKeyCasing::Camel => rust_field.to_lower_camel_case(),
        }
    }
}

/// Drop the workflow status field from a create-body entry list. The status
/// is WORKFLOW-MANAGED: the create DTO excludes it, and since #539 the DTOs
/// `deny_unknown_fields` — the pre-#539 "inert key on the wire" premise is
/// void and the key 422s. The row's state materializes via the DDL column
/// DEFAULT (the initial state), which is what the state-chip assertions pin.
pub(crate) fn strip_workflow_status(
    entries: Vec<(String, String)>,
    status_field: Option<&str>,
) -> Vec<(String, String)> {
    match status_field {
        Some(status) => entries
            .into_iter()
            .filter(|(field, _)| field != status)
            .collect(),
        None => entries,
    }
}

/// The URL route for a view, matching the route generator's svelte output
/// (`/{view-name-lowercase}`).
pub(crate) fn view_route(name: &str) -> String {
    format!("/{}", name.to_lowercase())
}

pub(crate) fn form_spec(c: &IfmlComponent) -> Option<&FormSpec> {
    match &c.spec {
        Some(ComponentSpec::Form(form)) => Some(form),
        _ => None,
    }
}

/// Resolve the API surface for an entity only when a schema for it exists in
/// the graph. Without schemas (`ifml-generate` without `--schemas`) this
/// returns `None`, which keeps API-dependent test kinds out of the output.
pub(crate) async fn schema_backed_api(
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
/// specs declare fields outside `fields`, so they are used as a fallback;
/// their per-field `values` supply the first valid value for
/// codelist/dropdown fields.
pub(crate) fn fixture_entries(c: &IfmlComponent) -> Vec<(String, String)> {
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
    let form_values: std::collections::HashMap<&str, &[String]> = form_spec(c)
        .map(|spec| {
            spec.fields
                .iter()
                .filter(|f| !f.values.is_empty())
                .map(|f| (f.name.as_str(), f.values.as_slice()))
                .collect()
        })
        .unwrap_or_default();
    names
        .iter()
        .filter(|f| f.as_str() != "id" && !f.ends_with("_id"))
        .map(|f| {
            let rust_type = types.get(f.as_str()).copied().unwrap_or("String");
            let value = match form_values.get(f.as_str()) {
                Some(values) if !values.is_empty() => format!("'{}'", js_string(&values[0])),
                _ => js_value_for_type(f, rust_type),
            };
            (f.clone(), value)
        })
        .collect()
}

/// First form field eligible for a round-trip modification: a plain string
/// field (skipping id/FK fields), with its fixture and updated values.
pub(crate) fn modifiable_field(
    c: &IfmlComponent,
    form: &FormSpec,
) -> Option<(String, String, String)> {
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

pub(crate) fn js_value_for_type(field: &str, rust_type: &str) -> String {
    let t = rust_type.to_ascii_lowercase();
    if t.contains("string") && field.to_ascii_lowercase().contains("email") {
        // Email inputs: prose fails the browser's type=email validation and
        // the client-validate check blocks the submit (the journey leg of
        // the gate caught it).
        return "'test@example.com'".to_string();
    }
    if t.contains("bool") {
        return "true".to_string();
    }
    if super::super::control_core::is_numeric_rust_type(&t) {
        return "42".to_string();
    }
    if t.contains("uuid") {
        // Random valid UUID literal (issue #558): 'Test <field>' 422s on
        // every uuid column — identity PKs included (#479 parity).
        return "`00000000-0000-4000-8000-${Math.random().toString(16).slice(2, 14).padStart(12, '0')}`"
            .to_string();
    }
    if t.contains("naivedate") || t.contains("date") && !t.contains("datetime") {
        // ISO date (issue #558): date columns reject prose.
        return "'2024-01-15'".to_string();
    }
    if t.contains("datetime") || t.contains("timestamp") {
        return "'2024-01-15T10:30:00Z'".to_string();
    }
    format!("'Test {field}'")
}

/// Resolve the schema node for an entity title (exact, then `+Type`).
pub(crate) async fn schema_for_entity(
    db: &dyn GraphQuerier,
    entity: &str,
) -> Option<codegraph_core::types::SchemaNode> {
    match db.get_schema(entity).await {
        Ok(Some(schema)) => Some(schema),
        Ok(None) => db.get_schema(&format!("{entity}Type")).await.ok()?,
        Err(_) => None,
    }
}

/// Whether a flat create body can satisfy the entity's required fields
/// (issue #558): a required entity-ref needs a real parent row, which a
/// flat body can never carry — seeds for such entities are DROPPED (never
/// a guaranteed-422 POST). Unknown entities (runs without schemas) are
/// conservatively creatable.
pub(crate) async fn required_refs_satisfiable(db: &dyn GraphQuerier, entity: Option<&str>) -> bool {
    let Some(entity) = entity else {
        return true;
    };
    let Some(schema) = schema_for_entity(db, entity).await else {
        return true;
    };
    let Ok(props) = db.get_properties(&schema.title).await else {
        return true;
    };
    !props.iter().any(|p| {
        p.is_required
            && p.name != "id"
            && matches!(
                p.effective_kind(),
                Some(codegraph_type_contracts::RefClassificationKind::EntityReference)
            )
    })
}

/// The first enum value of the codelist backing `field` on `entity`'s
/// schema, when the property references one. `None` keeps the generic
/// fixture value. Handles both JSON codelist `$ref` paths (`.../X.json`)
/// and mox enum targets (`PayRunStatus`, no path suffix).
pub(crate) async fn codelist_fixture_value(
    db: &dyn GraphQuerier,
    entity: Option<&str>,
    field: &str,
) -> Option<String> {
    let entity = entity?;
    let schema = schema_for_entity(db, entity).await?;
    let props = db.get_properties(&schema.title).await.ok()?;
    let prop = props.iter().find(|p| {
        p.name == field
            || p.rust_field_name == field
            || codegraph_naming::to_snake_case(&p.name) == codegraph_naming::to_snake_case(field)
    })?;
    if !matches!(
        prop.effective_kind(),
        Some(RefClassificationKind::CodelistReference) | Some(RefClassificationKind::CodelistCheck)
    ) {
        return None;
    }
    codelist_first_value(db, prop).await
}

/// The first enum value of a codelist-backed property, trying the
/// `$ref`/alias target first and the property name second (mox enums key
/// their `CodeList` by the enum name).
async fn codelist_first_value(db: &dyn GraphQuerier, prop: &PropertyNode) -> Option<String> {
    let mut names: Vec<String> = Vec::new();
    if let Some(target) = prop.ref_target.as_deref() {
        let leaf = target.rsplit('/').next().unwrap_or(target);
        names.push(leaf.trim_end_matches(".json").to_string());
    }
    names.push(prop.name.clone());
    names.push(prop.rust_field_name.clone());
    names.dedup();
    for name in names {
        if let Ok(values) = db.get_enum_values(&name).await
            && let Some(first) = values.into_iter().next()
        {
            return Some(first.value);
        }
    }
    None
}

/// Merge the entity's **required** schema properties into a fixture entry
/// list (P1). Component fields keep their existing values; required fields
/// the component does not surface are appended with a type-valid literal so
/// a typed create body does not 422 on a missing NOT NULL column.
///
/// Skipped:
/// - `id` and entity references (a required FK needs a real parent row —
///   the caller's `required_refs_satisfiable` gate drops those seeds);
/// - workflow-managed status fields (the create DTO excludes them);
/// - structured/value-object fields (their child DTOs are not synthesized
///   here; optional ones default, required ones keep the caller's gate).
pub(crate) async fn with_required_entries(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    entity: Option<&str>,
    entries: Vec<(String, String)>,
) -> Vec<(String, String)> {
    let Some(entity) = entity else {
        return entries;
    };
    let Some(schema) = schema_for_entity(db, entity).await else {
        return entries;
    };
    let MachineInfo {
        field: machine_field,
        initial: machine_initial,
    } = machine_info(db, &schema.title).await;
    let entity_cfg = config
        .domains
        .values()
        .find_map(|d| d.get_entity_config(&schema.title));
    let mut excluded: Vec<String> = entity_cfg
        .and_then(|ec| ec.workflow.as_ref())
        .map(|wf| {
            let mut fields = vec![wf.status_field.clone()];
            if let Some(approval) = &wf.approval_status_field {
                fields.push(approval.clone());
            }
            fields
        })
        .unwrap_or_default();
    if let Some(ec) = entity_cfg {
        excluded.extend(ec.dto.immutable_fields.iter().cloned());
    }
    let mut present: std::collections::HashSet<String> = entries
        .iter()
        .map(|(field, _)| codegraph_naming::to_snake_case(field))
        .collect();
    let props = db.get_properties(&schema.title).await.unwrap_or_default();
    let mut out = entries;
    for prop in &props {
        if !prop.is_required || prop.is_id {
            continue;
        }
        let field_def = resolve_field(prop);
        let key = &field_def.rust_field_name;
        if present.contains(key)
            || excluded
                .iter()
                .any(|f| codegraph_naming::to_snake_case(f) == codegraph_naming::to_snake_case(key))
        {
            continue;
        }
        // A required FK needs a dependency seed; leave it to the caller's
        // `required_refs_satisfiable` gate (which drops the seed).
        if matches!(
            prop.effective_kind(),
            Some(RefClassificationKind::EntityReference)
        ) {
            continue;
        }
        let value = property_fixture_value(
            db,
            prop,
            machine_field.as_deref(),
            machine_initial.as_deref(),
        )
        .await;
        if let Some(value) = value {
            present.insert(key.clone());
            out.push((key.clone(), value));
        }
    }
    out
}

struct MachineInfo {
    field: Option<String>,
    initial: Option<String>,
}

async fn machine_info(db: &dyn GraphQuerier, schema_title: &str) -> MachineInfo {
    let Some(surface) = DddDesignSurface::from_graph(db).await.ok() else {
        return MachineInfo {
            field: None,
            initial: None,
        };
    };
    let Some(machine) = surface.machine_for(schema_title) else {
        return MachineInfo {
            field: None,
            initial: None,
        };
    };
    MachineInfo {
        field: Some(codegraph_naming::to_snake_case(&machine.field)),
        initial: Some(machine.initial.clone()),
    }
}

/// A type-valid JS literal for one schema property (P1): codelist first
/// enum value (or the machine's initial state), wrapped-datatype format
/// samples, name heuristics for the crewbase wrappers, pattern samples,
/// then the pg/rust type sentinel.
async fn property_fixture_value(
    db: &dyn GraphQuerier,
    prop: &PropertyNode,
    machine_field: Option<&str>,
    machine_initial: Option<&str>,
) -> Option<String> {
    match prop.effective_kind() {
        Some(RefClassificationKind::CodelistReference)
        | Some(RefClassificationKind::CodelistCheck) => {
            // Array codelists project as child DTOs, not scalar values.
            if prop.is_array {
                return None;
            }
            if machine_field.is_some_and(|field| field == prop.rust_field_name)
                && let Some(initial) = machine_initial
            {
                return Some(format!("'{}'", js_string(initial)));
            }
            codelist_first_value(db, prop)
                .await
                .map(|value| format!("'{}'", js_string(&value)))
        }
        Some(RefClassificationKind::EntityReference)
        | Some(RefClassificationKind::ValueObject)
        | Some(RefClassificationKind::StructuredWrapper)
        | Some(RefClassificationKind::CompositeWrapper)
        | Some(RefClassificationKind::MediaWrapper) => None,
        _ => {
            if prop.is_array {
                // Required primitive arrays accept an empty Vec.
                return Some("[]".to_string());
            }
            Some(primitive_fixture_value(prop))
        }
    }
}

fn primitive_fixture_value(prop: &PropertyNode) -> String {
    if let Some(sample) = format_sample(prop.format.as_deref()) {
        return sample.to_string();
    }
    if let Some(sample) = name_sample(&prop.rust_field_name, &prop.rust_field_type) {
        return sample;
    }
    if let Some(sample) = pattern_sample(prop.pattern.as_deref()) {
        return sample.to_string();
    }
    match prop.rust_field_type.as_str() {
        "bool" => "true".to_string(),
        "i8" | "i16" | "i32" | "i64" | "i128" | "u8" | "u16" | "u32" | "u64" | "u128" | "usize"
        | "isize" => "42".to_string(),
        "f32" | "f64" => "42.5".to_string(),
        "rust_decimal::Decimal" | "Decimal" => "'4200'".to_string(),
        "chrono::NaiveDate" | "NaiveDate" => "'2024-01-15'".to_string(),
        "chrono::DateTime<chrono::Utc>" | "DateTime<Utc>" => "'2024-01-15T10:30:00Z'".to_string(),
        "Uuid" | "uuid::Uuid" => uuid_literal(),
        "serde_json::Value" => "{}".to_string(),
        _ => {
            if prop.pg_column_type.to_uppercase().starts_with("NUMERIC") {
                "'4200'".to_string()
            } else {
                format!("'Test {}'", prop.rust_field_name)
            }
        }
    }
}

fn format_sample(format: Option<&str>) -> Option<&'static str> {
    Some(match format? {
        "email" => "'test@example.com'",
        "uri" | "url" => "'https://example.com'",
        "uuid" => {
            "`00000000-0000-4000-8000-${Math.random().toString(16).slice(2, 14).padStart(12, '0')}`"
        }
        "date" => "'2024-01-15'",
        "date-time" | "datetime" | "timestamp" => "'2024-01-15T10:30:00Z'",
        "country" | "country-code" | "iso-3166" | "country_code" => "'NZ'",
        "language" | "language-code" | "iso-639" | "language_code" => "'en'",
        "jurisdiction" | "jurisdiction-code" | "jurisdiction_code" => "'NZ'",
        "currency" | "currency-code" | "currency_code" => "'NZD'",
        "phone" | "phone-number" => "'+64211234567'",
        "nzbn" => "'9429000000000'",
        "ird-number" | "ird_number" => "'123456789'",
        _ => return None,
    })
}

/// Name heuristics for wrapped datatypes the crews model declares without a
/// recognized `format` (country/language/jurisdiction/currency codes,
/// money-in-cents and basis points are the documented 422 class).
fn name_sample(field: &str, rust_type: &str) -> Option<String> {
    let numeric = matches!(
        rust_type,
        "rust_decimal::Decimal" | "Decimal" | "f32" | "f64"
    ) || rust_type.starts_with('i')
        || rust_type.starts_with('u');
    if field.contains("country") {
        return Some("'NZ'".to_string());
    }
    if field.contains("language") {
        return Some("'en'".to_string());
    }
    if field.contains("jurisdiction") {
        return Some("'NZ'".to_string());
    }
    if field.contains("currency") {
        return Some("'NZD'".to_string());
    }
    if field.contains("email") {
        return Some("'test@example.com'".to_string());
    }
    if field.ends_with("_cents")
        || field.contains("amount")
        || field.contains("basis_point")
        || field.ends_with("_rate")
        || field.contains("full_time_equivalent")
    {
        return Some(if numeric { "4200" } else { "'4200'" }.to_string());
    }
    None
}

fn pattern_sample(pattern: Option<&str>) -> Option<&'static str> {
    let pattern = pattern?;
    if pattern.contains("[A-Z]{2}") {
        return Some("'NZ'");
    }
    if pattern.contains("[a-z]{2}") {
        return Some("'en'");
    }
    if pattern.contains("\\d") || pattern.contains("[0-9]") {
        return Some("'123456'");
    }
    None
}

fn uuid_literal() -> String {
    "`00000000-0000-4000-8000-${Math.random().toString(16).slice(2, 14).padStart(12, '0')}`"
        .to_string()
}

/// The view-level or component-level `on save`/`on submit` navigation target:
/// `(target_view, bindings)`.
pub(crate) fn save_navigation_target(
    vc: &IfmlViewContainer,
    c: &IfmlComponent,
) -> Option<(String, std::collections::HashMap<String, String>)> {
    let is_save = |e: &IfmlEvent| e.event_type == "save" || e.event_type == "submit";
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
pub(crate) fn url_pattern(
    route: &str,
    binding: &std::collections::HashMap<String, String>,
) -> String {
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

pub(crate) fn binding_value_pattern(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        escape_regex(&trimmed[1..trimmed.len() - 1])
    } else {
        "[^&]+".to_string()
    }
}

/// URL pattern for navigation into a modal view: the `dialog=open` marker
/// is appended last, mirroring the route generator's `nav_url_expr` ordering.
pub(crate) fn url_pattern_with_dialog(
    route: &str,
    binding: &std::collections::HashMap<String, String>,
) -> String {
    if binding.is_empty() {
        return format!("{}\\?dialog=open", escape_regex(route));
    }
    format!("{}&dialog=open", url_pattern(route, binding))
}

pub(crate) fn escape_regex(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if "\\.+*?()|[]{}^$#".contains(ch) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

pub(crate) fn js_string(s: &str) -> String {
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

/// The inner text of a JS literal (`'draft'` → `draft`, `42` → `42`).
pub(crate) fn js_literal_inner(literal: &str) -> String {
    let trimmed = literal.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('\'') && trimmed.ends_with('\'') {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    }
}

pub(crate) fn entry_inner(entries: &[(String, String)], field: &str) -> Option<String> {
    entries
        .iter()
        .find(|(name, _)| name == field)
        .map(|(_, value)| js_literal_inner(value))
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod typed_fixture_tests {
    use super::*;

    #[test]
    fn wrapped_datatype_formats_seed_valid_samples() {
        assert_eq!(format_sample(Some("country-code")), Some("'NZ'"));
        assert_eq!(format_sample(Some("language-code")), Some("'en'"));
        assert_eq!(format_sample(Some("jurisdiction-code")), Some("'NZ'"));
        assert_eq!(format_sample(Some("currency-code")), Some("'NZD'"));
        assert_eq!(format_sample(Some("email")), Some("'test@example.com'"));
        assert_eq!(format_sample(Some("date")), Some("'2024-01-15'"));
        assert_eq!(
            format_sample(Some("date-time")),
            Some("'2024-01-15T10:30:00Z'")
        );
        assert_eq!(format_sample(None), None);
        assert_eq!(format_sample(Some("unrecognized")), None);
    }

    #[test]
    fn name_heuristics_cover_the_crewbase_wrappers() {
        assert_eq!(
            name_sample("country_of_residence", "String").as_deref(),
            Some("'NZ'")
        );
        assert_eq!(
            name_sample("preferred_language", "String").as_deref(),
            Some("'en'")
        );
        assert_eq!(
            name_sample("jurisdiction", "String").as_deref(),
            Some("'NZ'")
        );
        assert_eq!(
            name_sample("employer_superannuation_cents", "String").as_deref(),
            Some("'4200'")
        );
        assert_eq!(
            name_sample("basis_points", "rust_decimal::Decimal").as_deref(),
            Some("4200")
        );
        assert_eq!(
            name_sample("full_time_equivalent", "String").as_deref(),
            Some("'4200'")
        );
        assert_eq!(name_sample("status", "String"), None);
    }

    #[test]
    fn pattern_samples_are_valid_shapes() {
        assert_eq!(pattern_sample(Some("^[A-Z]{2}$")), Some("'NZ'"));
        assert_eq!(pattern_sample(Some("^[a-z]{2}$")), Some("'en'"));
        assert_eq!(pattern_sample(Some(r"^\d{9}$")), Some("'123456'"));
        assert_eq!(pattern_sample(Some("^[0-9]{3}$")), Some("'123456'"));
        assert_eq!(pattern_sample(None), None);
    }
}
