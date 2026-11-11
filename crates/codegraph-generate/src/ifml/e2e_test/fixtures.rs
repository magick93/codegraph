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

/// One required-FK dependency seed. Dependencies are ordered leaf-first
/// (a parent's own parents are created before it).
#[derive(Debug, Clone)]
pub struct FixtureDep {
    pub base_path: String,
    pub entries: Vec<(String, String)>,
    pub casing: DtoKeyCasing,
    /// The JS variable holding the created parent id (e.g. `dep_0_id`).
    pub id_var: String,
}

#[derive(Debug, Clone)]
pub struct Fixture {
    pub base_path: String,
    pub entries: Vec<(String, String)>,
    /// The DTO wire-key casing (#539 follow-through): the create/update
    /// DTOs `deny_unknown_fields`, so the JSON keys must match the serde
    /// contract — the schema property name (`submittedAt`) is NOT the key
    /// under the default snake casing (`submitted_at`).
    pub casing: DtoKeyCasing,
    /// Required-FK parent seeds, created before this fixture and wired into
    /// `entries` by `id_var`.
    pub deps: Vec<FixtureDep>,
}

impl Fixture {
    pub(crate) fn data_literal(&self) -> String {
        self.data_literal_with(None)
    }

    /// The fixture body, rewriting `dep_*_id` placeholders to the
    /// per-fixture JS variable prefix so two seeded fixtures in one test
    /// never redeclare the same dependency variable.
    fn data_literal_with(&self, prefix: Option<&str>) -> String {
        let inner = self
            .entries
            .iter()
            .map(|(k, v)| {
                let value = match prefix {
                    Some(prefix) if v.starts_with("dep_") && v.ends_with("_id") => {
                        format!("{prefix}_{v}")
                    }
                    _ => v.clone(),
                };
                format!("'{}': {value}", self.wire_key(k))
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!("{{ {inner} }}")
    }

    /// Render the dependency POSTs + this fixture's create as JS statements.
    /// Returns the variable name holding the created record response.
    pub(crate) fn setup_js(&self, var: &str, indent: &str) -> String {
        let mut out = String::new();
        for dep in &self.deps {
            let entries = Fixture {
                base_path: dep.base_path.clone(),
                entries: dep.entries.clone(),
                casing: dep.casing,
                deps: Vec::new(),
            };
            let dep_id = format!("{var}_{}", dep.id_var);
            out.push_str(&format!(
                "{indent}const {dep_id}_resp = await request.post(apiUrl('{}'), {{ data: {} }});\n",
                js_string(&dep.base_path),
                entries.data_literal_with(Some(var))
            ));
            out.push_str(&format!(
                "{indent}expect({dep_id}_resp.ok(), 'fixture dependency should be created').toBeTruthy();\n"
            ));
            out.push_str(&format!(
                "{indent}const {dep_id}_json = await {dep_id}_resp.json();\n"
            ));
            out.push_str(&format!(
                "{indent}const {dep_id} = {dep_id}_json.data?.id ?? {dep_id}_json.id;\n"
            ));
        }
        out.push_str(&format!(
            "{indent}const {var} = await request.post(apiUrl('{}'), {{ data: {} }});\n",
            js_string(&self.base_path),
            self.data_literal_with(Some(var))
        ));
        out
    }

    /// Dependency-aware create with an explicit headers expression (the
    /// journey specs carry persona keys). `json` binds the parsed body.
    pub(crate) fn setup_js_with_headers(
        &self,
        var: &str,
        indent: &str,
        headers: &str,
        json: bool,
    ) -> String {
        let mut out = String::new();
        for dep in &self.deps {
            let dep_fixture = Fixture {
                base_path: dep.base_path.clone(),
                entries: dep.entries.clone(),
                casing: dep.casing,
                deps: Vec::new(),
            };
            let dep_id = format!("{var}_{}", dep.id_var);
            out.push_str(&format!(
                "{indent}const {dep_id}_resp = await request.post(apiUrl('{}'), {{ headers: {headers}, data: {} }});\n",
                js_string(&dep.base_path),
                dep_fixture.data_literal_with(Some(var))
            ));
            out.push_str(&format!(
                "{indent}const {dep_id}_json = await {dep_id}_resp.json();\n"
            ));
            out.push_str(&format!(
                "{indent}const {dep_id} = {dep_id}_json.data?.id ?? {dep_id}_json.id;\n"
            ));
        }
        if json {
            out.push_str(&format!(
                "{indent}const {var}_resp = await request.post(apiUrl('{}'), {{ headers: {headers}, data: {} }});\n",
                js_string(&self.base_path),
                self.data_literal_with(Some(var))
            ));
            out.push_str(&format!("{indent}const {var} = await {var}_resp.json();\n"));
        } else {
            out.push_str(&format!(
                "{indent}const {var} = await request.post(apiUrl('{}'), {{ headers: {headers}, data: {} }});\n",
                js_string(&self.base_path),
                self.data_literal_with(Some(var))
            ));
        }
        out
    }

    /// Like [`Fixture::setup_js`], but binds the PARSED JSON body to `var`
    /// (the round-trip/workflow specs read `created.data?.id`).
    pub(crate) fn setup_js_json(&self, var: &str, indent: &str) -> String {
        let mut out = self.setup_js(&format!("{var}_resp"), indent);
        out.push_str(&format!("{indent}const {var} = await {var}_resp.json();\n"));
        out
    }

    /// The DTO serde key for a schema property name: the Rust field name
    /// (`to_snake_case`) verbatim under the snake casing, camelCased under
    /// the camel casing (the `rename_all = "camelCase"` contract).
    fn wire_key(&self, schema_field: &str) -> String {
        wire_key_for(schema_field, self.casing)
    }
}

/// The DTO serde key for a schema property name under the given casing.
#[must_use]
pub(crate) fn wire_key_for(schema_field: &str, casing: DtoKeyCasing) -> String {
    let rust_field = codegraph_naming::to_snake_case(schema_field);
    match casing {
        DtoKeyCasing::Snake => rust_field,
        DtoKeyCasing::Camel => rust_field.to_lower_camel_case(),
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

/// Resolve the schema title a required entity-ref points at (`$ref` leaf
/// stem, then `+Type`).
async fn ref_target_title(db: &dyn GraphQuerier, prop: &PropertyNode) -> Option<String> {
    let target = prop.ref_target.as_deref()?;
    let leaf = target.rsplit('/').next().unwrap_or(target);
    let leaf = leaf.rsplit("::").next().unwrap_or(leaf);
    let stem = leaf.trim_end_matches(".json").trim_end_matches(".schema");
    if let Some(schema) = schema_for_entity(db, stem).await {
        return Some(schema.title);
    }
    // `$ref` paths carry the file stem (`group` / `group.schema`), while the
    // schema title is the PascalCase type (`GroupType`). Try that spelling,
    // then fall back to a table/rel-path scan.
    let pascal = codegraph_naming::to_pascal_case(stem);
    if let Some(schema) = schema_for_entity(db, &pascal).await {
        return Some(schema.title);
    }
    db.list_schemas(None)
        .await
        .ok()?
        .into_iter()
        .find(|schema| {
            schema.pg_table_name == stem
                || schema.rel_path.ends_with(target)
                || schema.rel_path.ends_with(&format!("{stem}.json"))
        })
        .map(|schema| schema.title)
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
    let classified = matches!(
        prop.effective_kind(),
        Some(RefClassificationKind::CodelistReference) | Some(RefClassificationKind::CodelistCheck)
    );
    if !classified && !ref_target_is_codelist(db, prop).await {
        return None;
    }
    codelist_first_value(db, prop).await
}

/// Whether a property's `$ref` target resolves to a codelist SchemaNode.
/// Graph fixtures (and some import paths) carry the ref without the
/// classification kind, so the target schema is the authoritative signal.
async fn ref_target_is_codelist(db: &dyn GraphQuerier, prop: &PropertyNode) -> bool {
    let Some(target) = prop.ref_target.as_deref() else {
        return false;
    };
    let leaf = target.rsplit('/').next().unwrap_or(target);
    let stem = leaf.trim_end_matches(".json");
    matches!(db.get_schema(stem).await, Ok(Some(schema)) if schema.is_codelist)
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
/// - `id` and entity references (a required FK is appended later, with its
///   parent seed, by [`add_required_refs`]);
/// - workflow-managed status fields (the create DTO excludes them);
/// - structured/value-object fields (their child DTOs are not synthesized
///   here; optional ones default, required ones keep the caller's gate).
pub(crate) async fn with_required_entries(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    entity: Option<&str>,
    entries: Vec<(String, String)>,
    casing: DtoKeyCasing,
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
        // A required FK needs a dependency seed; `add_required_refs` appends
        // the FK entry and the parent seed after this pass.
        if matches!(
            prop.effective_kind(),
            Some(RefClassificationKind::EntityReference)
        ) {
            continue;
        }
        // A required structured wrapper is a nested object on the create
        // body, not a scalar literal. Synthesize its required members so the
        // typed DTO does not 422 on a missing JSONB value.
        if matches!(
            prop.effective_kind(),
            Some(RefClassificationKind::StructuredWrapper)
        ) {
            if let Some(value) = structured_wrapper_value(db, prop, casing).await {
                present.insert(key.clone());
                out.push((key.clone(), value));
            }
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

/// A required structured wrapper's nested object literal, seeded with its
/// required members (codelists use their first enum value).
async fn structured_wrapper_value(
    db: &dyn GraphQuerier,
    prop: &PropertyNode,
    casing: DtoKeyCasing,
) -> Option<String> {
    let title = ref_target_title(db, prop).await?;
    let schema = schema_for_entity(db, &title).await?;
    let props = db.get_properties(&schema.title).await.ok()?;
    let mut members = Vec::new();
    for member in props.iter().filter(|p| p.is_required && !p.is_id) {
        if matches!(
            member.effective_kind(),
            Some(RefClassificationKind::EntityReference)
        ) {
            continue;
        }
        let key = wire_key_for(&resolve_field(member).rust_field_name, casing);
        let value = match codelist_first_value(db, member).await {
            Some(code) => format!("'{}'", js_string(&code)),
            None => js_value_for_type(&member.name, &member.rust_field_type),
        };
        members.push(format!("'{key}': {value}"));
    }
    Some(format!("{{ {} }}", members.join(", ")))
}

/// Required-FK additions: child FK `(field, js id variable)` entries plus the
/// parent seeds (leaf-first).
#[derive(Debug, Default)]
pub(crate) struct RefPlan {
    pub entries: Vec<(String, String)>,
    pub deps: Vec<FixtureDep>,
}

/// Plan the required-FK closure for a fixture. Parents are seeded through
/// their own create endpoint and their created id is wired into the child
/// body. Cycles are broken at the repeated title (the FK stays omitted, the
/// same fail-closed shape as an uncreatable parent) and the closure is
/// depth-limited to 4.
///
/// `allow(too_many_arguments)`: the recursive planner threads the graph,
/// config, API version, casing, name counter, and cycle stack explicitly —
/// bundling them in a context struct would only move the same fields.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn plan_required_refs(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    api_version: &str,
    entity: Option<&str>,
    existing: &[(String, String)],
    casing: DtoKeyCasing,
    counter: &mut usize,
    stack: &mut Vec<String>,
) -> RefPlan {
    Box::pin(plan_required_refs_inner(
        db,
        config,
        api_version,
        entity,
        existing,
        casing,
        counter,
        stack,
    ))
    .await
}

#[allow(clippy::too_many_arguments)]
async fn plan_required_refs_inner(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    api_version: &str,
    entity: Option<&str>,
    existing: &[(String, String)],
    casing: DtoKeyCasing,
    counter: &mut usize,
    stack: &mut Vec<String>,
) -> RefPlan {
    let mut plan = RefPlan::default();
    let Some(entity) = entity else {
        return plan;
    };
    if stack.len() >= 4 {
        return plan;
    }
    let Some(schema) = schema_for_entity(db, entity).await else {
        return plan;
    };
    let Ok(props) = db.get_properties(&schema.title).await else {
        return plan;
    };
    let present: std::collections::HashSet<String> = existing
        .iter()
        .map(|(field, _)| codegraph_naming::to_snake_case(field))
        .collect();
    for prop in &props {
        if !prop.is_required || prop.is_id {
            continue;
        }
        if !matches!(
            prop.effective_kind(),
            Some(RefClassificationKind::EntityReference)
        ) {
            continue;
        }
        let fk_field = resolve_field(prop).rust_field_name;
        if present.contains(&fk_field) {
            continue;
        }
        let Some(target_title) = ref_target_title(db, prop).await else {
            continue;
        };
        if stack.iter().any(|s| s == &target_title) {
            continue;
        }
        let Some(api) = resolve_entity_api(db, config, &target_title, api_version).await else {
            continue;
        };
        if !api.has_create {
            continue;
        }
        let parent_entries =
            with_required_entries(db, config, Some(&target_title), Vec::new(), casing).await;
        stack.push(target_title.clone());
        let parent_plan = plan_required_refs(
            db,
            config,
            api_version,
            Some(&target_title),
            &parent_entries,
            casing,
            counter,
            stack,
        )
        .await;
        stack.pop();

        let mut entries = parent_entries;
        entries.extend(parent_plan.entries);
        let id_var = format!("dep_{}_id", *counter);
        *counter += 1;
        plan.deps.extend(parent_plan.deps);
        plan.deps.push(FixtureDep {
            base_path: api.base_path,
            entries,
            casing,
            id_var: id_var.clone(),
        });
        plan.entries.push((fk_field, id_var));
    }
    plan
}

/// Append the required-FK closure to an already-built fixture.
pub(crate) async fn add_required_refs(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    api_version: &str,
    entity: Option<&str>,
    mut fixture: Fixture,
) -> Fixture {
    let mut counter = 0usize;
    let mut stack: Vec<String> = entity.map(str::to_string).into_iter().collect();
    let plan = plan_required_refs(
        db,
        config,
        api_version,
        entity,
        &fixture.entries,
        fixture.casing,
        &mut counter,
        &mut stack,
    )
    .await;
    fixture.entries.extend(plan.entries);
    fixture.deps = plan.deps;
    fixture
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
    let codelist = matches!(
        prop.effective_kind(),
        Some(RefClassificationKind::CodelistReference) | Some(RefClassificationKind::CodelistCheck)
    ) || ref_target_is_codelist(db, prop).await;
    if codelist {
        // Array codelists project as child DTOs, not scalar values.
        if prop.is_array {
            return None;
        }
        if machine_field.is_some_and(|field| field == prop.rust_field_name)
            && let Some(initial) = machine_initial
        {
            return Some(format!("'{}'", js_string(initial)));
        }
        return codelist_first_value(db, prop)
            .await
            .map(|value| format!("'{}'", js_string(&value)));
    }
    match prop.effective_kind() {
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

    fn test_schema(title: &str, table: &str) -> codegraph_core::types::SchemaNode {
        codegraph_core::types::SchemaNode {
            namespace: None,
            schema_id: format!("id:{title}"),
            title: title.to_string(),
            access: None,
            annotations: None,
            description: None,
            schema_type: "object".to_string(),
            classification: "entity".to_string(),
            domain: Some("hr".to_string()),
            rel_path: format!("{title}.json"),
            pg_type: "UUID".to_string(),
            rust_type: "Uuid".to_string(),
            sea_orm_type: "Uuid".to_string(),
            rust_type_name: title.to_string(),
            pg_table_name: table.to_string(),
            api_path_segment: table.to_string(),
            parent_schema: None,
            is_entity: true,
            is_codelist: false,
            is_primitive_wrapper: false,
            has_all_of: false,
            has_one_of: false,
            has_any_of: false,
            has_definitions: false,
            custom_annotations: Default::default(),
        }
    }

    fn test_prop(name: &str, required: bool) -> codegraph_core::types::PropertyNode {
        codegraph_core::types::PropertyNode {
            name: name.to_string(),
            prop_type: "string".into(),
            description: None,
            format: None,
            is_required: required,
            is_nullable: !required,
            is_id: false,
            type_expr: None,
            is_array: false,
            min_items: None,
            max_items: None,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: codegraph_naming::to_snake_case(name),
            pg_column_type: "TEXT".into(),
            rust_field_name: codegraph_naming::to_snake_case(name),
            rust_field_type: "String".into(),
            sea_orm_type: "String".into(),
            render_strategy: "scalar".into(),
            ref_target: None,
            classification: None,
            projection: None,
            classification_kind: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
        }
    }

    fn entity_ref_prop(name: &str) -> codegraph_core::types::PropertyNode {
        let mut prop = test_prop(name, true);
        prop.pg_column_name = format!("{}_id", codegraph_naming::to_snake_case(name));
        prop.rust_field_name = format!("{}_id", codegraph_naming::to_snake_case(name));
        prop.ref_target = Some("hr/json/group.schema.json".to_string());
        prop.classification_kind = Some(RefClassificationKind::EntityReference);
        prop
    }

    #[tokio::test]
    async fn required_structured_wrappers_synthesize_a_nested_object() {
        let mut wrapper = test_schema("AddressType", "address");
        wrapper.classification = "value_object".to_string();
        wrapper.is_entity = false;
        let mut address = test_prop("address", true);
        address.ref_target = Some("hr/json/address.schema.json".to_string());
        address.classification_kind = Some(RefClassificationKind::StructuredWrapper);
        let engine = codegraph_core::mock::MockEngine::builder()
            .with_schema(test_schema("PersonType", "person"))
            .with_properties("PersonType", vec![test_prop("name", true), address])
            .with_schema(wrapper)
            .with_properties(
                "AddressType",
                vec![test_prop("street", true), test_prop("city", true)],
            )
            .build();
        let config: DomainConfig = toml::from_str(
            r#"
[domains.hr]
label = "HR"
schema_dir = "hr"
postgres_schema = "hr"
entities = ["PersonType"]
"#,
        )
        .unwrap();
        let entries = with_required_entries(
            &engine,
            &config,
            Some("PersonType"),
            Vec::new(),
            DtoKeyCasing::Snake,
        )
        .await;
        let address = entries
            .iter()
            .find(|(field, _)| field == "address")
            .map(|(_, value)| value.clone())
            .expect("structured wrapper entry");
        assert!(address.contains("'street'"), "{address}");
        assert!(address.contains("'city'"), "{address}");
    }

    #[tokio::test]
    async fn required_refs_seed_the_parent_and_wire_the_fk() {
        let engine = codegraph_core::mock::MockEngine::builder()
            .with_schema(test_schema("GroupType", "group"))
            .with_properties("GroupType", vec![test_prop("name", true)])
            .with_schema(test_schema("ChildType", "child"))
            .with_properties(
                "ChildType",
                vec![test_prop("title", true), entity_ref_prop("group")],
            )
            .build();
        let config: DomainConfig = toml::from_str(
            r#"
[domains.hr]
label = "HR"
schema_dir = "hr"
postgres_schema = "hr"
entities = ["GroupType", "ChildType"]
"#,
        )
        .unwrap();
        let fixture = add_required_refs(
            &engine,
            &config,
            "v1",
            Some("ChildType"),
            Fixture {
                base_path: "/api/v1/hr/child".to_string(),
                entries: vec![("title".to_string(), "'Test title'".to_string())],
                casing: DtoKeyCasing::Snake,
                deps: Vec::new(),
            },
        )
        .await;
        assert_eq!(
            fixture.deps.len(),
            1,
            "the required FK must produce one parent seed"
        );
        assert_eq!(fixture.deps[0].base_path, "/api/v1/hr/group");
        assert!(
            fixture.deps[0]
                .entries
                .iter()
                .any(|(field, _)| field == "name"),
            "the parent seed must carry its required fields: {:?}",
            fixture.deps[0].entries
        );
        assert!(
            fixture
                .entries
                .iter()
                .any(|(field, value)| field == "group_id" && value == "dep_0_id"),
            "the child body must reference the created parent id: {:?}",
            fixture.entries
        );
        let js = fixture.setup_js("fixture", "");
        assert!(js.contains("apiUrl('/api/v1/hr/group')"), "{js}");
        assert!(js.contains("const fixture_dep_0_id"), "{js}");
        assert!(
            js.contains("'group_id': fixture_dep_0_id"),
            "the child body must reference the namespaced id:\n{js}"
        );
    }
}
