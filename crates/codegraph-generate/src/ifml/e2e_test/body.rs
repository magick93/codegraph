//! The IFML e2e create-body projection (issues #806/#808/#809).
//!
//! Every seeded spec (auth `beforeAll`, click-through, round-trip, workflow,
//! ux, action, journey) posts a create body. Building that body from the
//! component's DSL `fields` alone is wrong three ways, and each way 422s the
//! seed (so `seededId` binds `"undefined"` and every downstream assertion
//! fails on an unrelated screen):
//!
//! - the create DTO is `#[serde(deny_unknown_fields)]` over the PROJECTED
//!   Rust field names, while the DSL speaks the model spelling
//!   (`workerTypeCode` → `worker_type`, `vendor` → `vendor_id`), and it omits
//!   workflow-managed / media fields the DSL may name;
//! - a composite wrapper (`Money amount`) is several columns in the DTO
//!   (`amount_cents`, `amount_currency`) and no `amount` field at all;
//! - every required DTO member has no serde default — identity / natural-key
//!   parts included (issue #522: a composite key's `code`/`jurisdiction`/
//!   `rule_id` are required create-body members) — so a body assembled from
//!   the form fields alone is missing fields the model never surfaced.
//!
//! [`plan_seed`] therefore derives the body from the DTO plane's OWN create
//! surface ([`build_dto_context`]) — never a second copy of the DTO rules —
//! keeps the DSL values where the DTO declares the field, samples every other
//! REQUIRED field, drops what the DTO does not declare, and wires required
//! entity references to freshly seeded parents (graph edges, then the shared
//! name-convention resolver, [`crate::fk_convention`]).
//!
//! A required reference with no creatable target (or a cycle) is a
//! [`SeedGap`]: reported and the seeded spec omitted, never a seed the
//! generator knows will 4xx.
//!
//! `None` from [`plan_seed`] means the DTO surface is unavailable (no
//! schema/domain/context) — the caller falls back to the historical
//! prop-driven path, so an ifml-only run keeps working.

use std::collections::{HashMap, HashSet};

use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::{PropertyNode, SchemaNode, resolve_field};
use codegraph_type_contracts::RefClassificationKind;

use super::super::api_paths::resolve_entity_api;
use crate::ddd::dto::build_dto_context;
use crate::fk_convention::resolve_convention_ref;
use crate::project_config::DtoKeyCasing;

use super::fixtures::{
    FixtureDep, SeedGap, machine_info, property_fixture_value, ref_target_title, sample_for_column,
    schema_for_entity, structured_wrapper_value,
};

/// The maximum required-FK closure depth (the historical `plan_required_refs`
/// bound: self/cyclic references terminate fail-closed).
const MAX_REF_DEPTH: usize = 4;

/// One seed plan: the create body, its parent seeds (leaf-first), and the
/// required fields that could not be seeded.
#[derive(Debug, Default)]
pub(crate) struct SeedPlan {
    pub entries: Vec<(String, String)>,
    pub deps: Vec<FixtureDep>,
    pub gaps: Vec<SeedGap>,
}

/// One create-request field's shape (the subset the seed planner needs).
struct CreateField {
    name: String,
    rust_type: String,
    is_required: bool,
    is_entity_ref: bool,
    is_array: bool,
}

/// Build a seed plan for `entity` from the component's DSL entries.
///
/// `dsl_entries` arrive in the model spelling (the form/`fields:` names) and
/// may already carry this generator's codelist overrides; the projection is
/// idempotent over an already-projected key. `None` when the DTO plane cannot
/// resolve the entity (no schema, no owning domain, or a DTO build error) —
/// the caller then uses the legacy prop-driven path.
pub(super) async fn plan_seed(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    api_version: &str,
    entity: Option<&str>,
    dsl_entries: Vec<(String, String)>,
    casing: DtoKeyCasing,
) -> Option<SeedPlan> {
    let entity = entity?;
    let mut stack: Vec<String> = vec![entity.to_string()];
    let mut counter = 0usize;
    Box::pin(plan_seed_inner(
        db,
        config,
        api_version,
        entity,
        dsl_entries,
        casing,
        &mut counter,
        &mut stack,
    ))
    .await
}

#[allow(clippy::too_many_arguments)]
async fn plan_seed_inner(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    api_version: &str,
    entity: &str,
    dsl_entries: Vec<(String, String)>,
    casing: DtoKeyCasing,
    counter: &mut usize,
    stack: &mut Vec<String>,
) -> Option<SeedPlan> {
    let schema = schema_for_entity(db, entity).await?;
    let domain = schema.domain.clone()?;
    let ctx = build_dto_context(db, &schema.title, &domain, config)
        .await
        .ok()?;
    let props = db.get_properties(&schema.title).await.unwrap_or_default();

    // ── The create surface: exactly what `domain_types/dto_create.tera`
    // emits — every DTO field except the surrogate `id` and the
    // workflow-managed status/approval fields. Media references are child
    // DTOs, and a create child is always `Option` + `#[serde(default)]`, so
    // an omitted child is valid (the upload handler owns a through-app
    // reference anyway).
    let create_fields: Vec<CreateField> = ctx
        .fields
        .iter()
        .filter(|f| f.name != "id")
        .filter(|f| {
            !ctx.workflow_excluded_fields.iter().any(|excluded| {
                codegraph_naming::to_snake_case(excluded)
                    == codegraph_naming::to_snake_case(&f.name)
            })
        })
        .map(|f| CreateField {
            name: f.name.clone(),
            rust_type: f.rust_type.clone(),
            // A value-object reference that resolves to an ENTITY projects
            // as an FK column but `resolve_dto_fields` marks it optional; the
            // COLUMN is NOT NULL when the modeled feature is required, so the
            // seed must supply it (the `RsvpType.event` shape).
            is_required: f.is_required || required_vo_ref(&props, &f.name),
            is_entity_ref: f.is_entity_ref,
            is_array: f.is_array,
        })
        .collect();
    if create_fields.is_empty() {
        return Some(SeedPlan::default());
    }
    let by_name: HashMap<String, usize> = create_fields
        .iter()
        .enumerate()
        .map(|(i, f)| (f.name.clone(), i))
        .collect();

    // Composite wrappers: one model field → several DTO columns.
    let composites = composite_columns(db, &schema, &props).await;
    let machine = machine_info(db, &schema.title).await;

    // Which create fields are references (seeded through a parent POST) —
    // classified ONCE: the DTO's own kind, plus the name-convention FK shape
    // (`vendor_id` with no `$ref`/edge, issue #806).
    let mut refs: Vec<RefField> = Vec::with_capacity(create_fields.len());
    for field in &create_fields {
        refs.push(
            classify_ref(
                db,
                prop_for_dto_name(&props, &field.name),
                &domain,
                &schema.title,
                &field.name,
                field.is_entity_ref,
            )
            .await,
        );
    }

    // ── 1. Project the DSL entries onto the create surface, preserving
    // their order (the pre-#806 bodies keep their exact key order; only the
    // shapes that were wrong move).
    let mut plan = SeedPlan {
        entries: project_entries(
            dsl_entries,
            &by_name,
            &create_fields,
            &props,
            &composites,
            &refs,
        ),
        deps: Vec::new(),
        gaps: Vec::new(),
    };
    let mut present: HashSet<String> = plan.entries.iter().map(|(f, _)| f.clone()).collect();

    // ── 2. Every required member the body does not carry yet: identity /
    // natural-key parts included (they have no serde default).
    for (index, field) in create_fields.iter().enumerate() {
        if present.contains(&field.name) {
            continue;
        }
        if !matches!(refs[index], RefField::No) {
            continue; // step 3 owns references
        }
        if field.is_array {
            if field.is_required {
                present.insert(field.name.clone());
                plan.entries.push((field.name.clone(), "[]".to_string()));
            }
            continue;
        }
        match scalar_sample(db, &props, &composites, field, &machine, casing).await {
            Some(value) => {
                present.insert(field.name.clone());
                plan.entries.push((field.name.clone(), value));
            }
            None if field.is_required => {
                plan.gaps.push(SeedGap::new(
                    schema.title.clone(),
                    field.name.clone(),
                    "no type-valid fixture value for a required field",
                ));
            }
            None => {}
        }
    }

    // ── 3. Required scalar references: seed the parent through its own
    // create endpoint and wire the id. Optional references are left null
    // (a valid body — never a random uuid).
    for (index, field) in create_fields.iter().enumerate() {
        if field.is_array || !field.is_required {
            continue;
        }
        let target = match &refs[index] {
            RefField::No => continue,
            RefField::Target(target) => target.clone(),
            RefField::Unresolved => {
                plan.gaps.push(SeedGap::new(
                    schema.title.clone(),
                    field.name.clone(),
                    "no resolvable entity target",
                ));
                continue;
            }
        };
        if plan.entries.iter().any(|(f, _)| f == &field.name) {
            continue;
        }
        if stack.contains(&target) {
            plan.gaps.push(SeedGap::new(
                schema.title.clone(),
                field.name.clone(),
                format!("required reference to {target} closes a seed cycle"),
            ));
            continue;
        }
        let Some(api) = resolve_entity_api(db, config, &target, api_version).await else {
            plan.gaps.push(SeedGap::new(
                schema.title.clone(),
                field.name.clone(),
                format!("{target} has no API surface"),
            ));
            continue;
        };
        if !api.has_create {
            plan.gaps.push(SeedGap::new(
                schema.title.clone(),
                field.name.clone(),
                format!("{target} exposes no create operation"),
            ));
            continue;
        }
        if stack.len() >= MAX_REF_DEPTH {
            plan.gaps.push(SeedGap::new(
                schema.title.clone(),
                field.name.clone(),
                format!("required-reference chain deeper than {MAX_REF_DEPTH}"),
            ));
            continue;
        }
        stack.push(target.clone());
        let parent = Box::pin(plan_seed_inner(
            db,
            config,
            api_version,
            &target,
            Vec::new(),
            casing,
            counter,
            stack,
        ))
        .await;
        stack.pop();
        let Some(parent) = parent else {
            plan.gaps.push(SeedGap::new(
                schema.title.clone(),
                field.name.clone(),
                format!("{target} has no DTO surface to seed"),
            ));
            continue;
        };
        if let Some(gap) = parent.gaps.first() {
            // A parent that cannot be created cannot satisfy the FK.
            plan.gaps.push(SeedGap::new(
                schema.title.clone(),
                field.name.clone(),
                format!("{target} is unseedable: {gap}"),
            ));
            continue;
        }
        let id_var = format!("dep_{}_id", *counter);
        *counter += 1;
        plan.deps.extend(parent.deps);
        plan.deps.push(FixtureDep {
            base_path: api.base_path,
            entries: parent.entries,
            casing,
            id_var: id_var.clone(),
        });
        plan.entries.push((field.name.clone(), id_var));
    }
    Some(plan)
}

/// The entity plane's create-body supplement (issue #324).
///
/// The entity ux spec renders its fixture body in Tera from the ui field set
/// (`collect_ui_fields`), which can miss create-DTO members (the W3C-credential
/// `issuance_date`/`issuer`/`type` shape). This runs the SAME seed planner the
/// IFML plane uses — the create DTO's own surface via `build_dto_context` —
/// and returns, for every required field the ui body does NOT already carry
/// (`covered`, snake spellings):
///
/// - `entries`: a type-valid JS literal per uncovered required scalar/array;
/// - `gaps`: every uncovered required field the entity spec cannot seed (no
///   sample, an unseedable reference, or a reference the entity plane's own
///   dependency setup does not wire — those must come from `covered`).
///
/// `None` when the DTO surface is unavailable (no schema/domain/context).
pub(crate) async fn create_body_supplement(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    api_version: &str,
    entity: &str,
    covered: &HashSet<String>,
) -> Option<(Vec<(String, String)>, Vec<SeedGap>)> {
    let plan = plan_seed(
        db,
        config,
        api_version,
        Some(entity),
        Vec::new(),
        DtoKeyCasing::Snake,
    )
    .await?;
    let is_covered = |name: &str| covered.contains(&codegraph_naming::to_snake_case(name));
    let dep_vars: HashSet<&str> = plan.deps.iter().map(|d| d.id_var.as_str()).collect();
    let mut entries = Vec::new();
    let mut gaps: Vec<SeedGap> = plan
        .gaps
        .into_iter()
        .filter(|gap| !is_covered(&gap.field))
        .collect();
    for (name, value) in plan.entries {
        if is_covered(&name) {
            continue;
        }
        if dep_vars.contains(value.as_str()) {
            // A required reference the entity plane's dependency setup does
            // not wire: the spec has no seeded id to post.
            gaps.push(SeedGap::new(
                entity,
                name,
                "required reference the entity spec's dependency setup does not seed",
            ));
            continue;
        }
        entries.push((name, value));
    }
    Some((entries, gaps))
}

/// Project the DSL entries onto the create surface: composite wrappers
/// expand to their collapsed columns, a reference's literal placeholder is
/// dropped (the required-ref pass seeds the parent), and a key the create DTO
/// does not declare is dropped — it would 422 under `deny_unknown_fields`.
fn project_entries(
    dsl_entries: Vec<(String, String)>,
    by_name: &HashMap<String, usize>,
    create_fields: &[CreateField],
    props: &[PropertyNode],
    composites: &HashMap<String, Vec<CompositeColumn>>,
    refs: &[RefField],
) -> Vec<(String, String)> {
    let mut entries: Vec<(String, String)> = Vec::new();
    for (raw, value) in dsl_entries {
        let snake = codegraph_naming::to_snake_case(&raw);
        if let Some(cols) = composites.get(&snake) {
            // A wrapper literal (`{ amount, currency }`) is not a wire shape:
            // the DTO takes the collapsed columns.
            for col in cols {
                push_entry(
                    &mut entries,
                    col.name.clone(),
                    sample_for_column(&col.name, &col.rust_type),
                );
            }
            continue;
        }
        let Some(index) = project_key(by_name, props, &snake) else {
            // The create DTO does not declare it (`deny_unknown_fields` ⇒ a
            // guaranteed 422): drop it rather than post it.
            continue;
        };
        let field = &create_fields[index];
        if field.is_entity_ref || !matches!(refs[index], RefField::No) {
            // A reference is never a literal: the placeholder the DSL
            // carried (`vendorId` → a random uuid) would fail the FK — and
            // for a name-convention FK the DTO kind says "plain scalar", so
            // the classification above is what knows better. The required-FK
            // pass seeds the parent and wires the id.
            continue;
        }
        push_entry(&mut entries, field.name.clone(), value);
    }
    entries
}

/// The create-field index a DSL key projects onto: an exact create-field
/// match, or the prop's `resolve_field` projection (`vendor` → `vendor_id`).
/// `None` when the create DTO does not declare it.
fn project_key(
    by_name: &HashMap<String, usize>,
    props: &[PropertyNode],
    snake: &str,
) -> Option<usize> {
    if let Some(index) = by_name.get(snake) {
        return Some(*index);
    }
    let prop = prop_for_dto_name(props, snake)?;
    let projected = resolve_field(prop).rust_field_name;
    by_name.get(&projected).copied()
}

/// The graph property behind a create-DTO field name (the scalar reference
/// arm suffixes `_id`; a composite column has none).
fn prop_for_dto_name<'a>(props: &'a [PropertyNode], field: &str) -> Option<&'a PropertyNode> {
    props.iter().find(|p| {
        p.rust_field_name == field
            || resolve_field(p).rust_field_name == field
            || codegraph_naming::to_snake_case(&p.name) == field
    })
}

/// Whether an optional-in-the-DTO FK column is actually a REQUIRED column
/// (a value-object reference that resolves to an entity keeps the modeled
/// requiredness on the column while the DTO marks it `Option`).
fn required_vo_ref(props: &[PropertyNode], dto_name: &str) -> bool {
    prop_for_dto_name(props, dto_name)
        .filter(|p| {
            matches!(p.effective_kind(), Some(RefClassificationKind::ValueObject)) && !p.is_array
        })
        .map(|p| p.is_required)
        .unwrap_or(false)
}

struct CompositeColumn {
    name: String,
    rust_type: String,
}

/// Composite-wrapper expansions keyed by the snake-cased model spelling and
/// the stored field name (a DSL `amount` and an `amount` prop both find it).
async fn composite_columns(
    db: &dyn GraphQuerier,
    schema: &SchemaNode,
    props: &[PropertyNode],
) -> HashMap<String, Vec<CompositeColumn>> {
    let mut out: HashMap<String, Vec<CompositeColumn>> = HashMap::new();
    for prop in props {
        if !matches!(
            prop.effective_kind(),
            Some(RefClassificationKind::CompositeWrapper)
        ) {
            continue;
        }
        let Ok(cols) = db.get_composite_columns(&prop.name, &schema.title).await else {
            continue;
        };
        let mapped: Vec<CompositeColumn> = cols
            .into_iter()
            .map(|col| CompositeColumn {
                name: format!("{}{}", prop.rust_field_name, col.suffix),
                rust_type: col.dto_rust_type.unwrap_or(col.rust_type),
            })
            .collect();
        if mapped.is_empty() {
            continue;
        }
        for key in [
            codegraph_naming::to_snake_case(&prop.name),
            codegraph_naming::to_snake_case(&prop.rust_field_name),
        ] {
            out.entry(key).or_insert_with(|| {
                mapped
                    .iter()
                    .map(|c| CompositeColumn {
                        name: c.name.clone(),
                        rust_type: c.rust_type.clone(),
                    })
                    .collect()
            });
        }
    }
    out
}

/// The value for one required scalar create field: the shared typed sampler
/// (codelist first value / machine initial / wrapped-datatype sample), a
/// structured wrapper's nested object, or a composite column's sample.
async fn scalar_sample(
    db: &dyn GraphQuerier,
    props: &[PropertyNode],
    composites: &HashMap<String, Vec<CompositeColumn>>,
    field: &CreateField,
    machine: &super::fixtures::MachineInfo,
    casing: DtoKeyCasing,
) -> Option<String> {
    if let Some(prop) = prop_for_dto_name(props, &field.name) {
        if matches!(
            prop.effective_kind(),
            Some(RefClassificationKind::StructuredWrapper)
        ) {
            return structured_wrapper_value(db, prop, casing).await;
        }
        if let Some(value) = property_fixture_value(
            db,
            prop,
            machine.field.as_deref(),
            machine.initial.as_deref(),
        )
        .await
        {
            return Some(value);
        }
    }
    // A composite column carries no property of its own.
    if composites
        .values()
        .any(|cols| cols.iter().any(|c| c.name == field.name))
    {
        return Some(sample_for_column(&field.name, &field.rust_type));
    }
    None
}

/// Whether a create field must be satisfied by a seeded parent.
#[derive(Debug, Clone, PartialEq, Eq)]
enum RefField {
    /// Known reference with a resolved entity target.
    Target(String),
    /// A reference the model declares (graph kind) whose target resolves to
    /// nothing — a named gap, never a literal.
    Unresolved,
    /// Not a reference: sample a value.
    No,
}

/// Classify one create field: the DTO plane says it is an entity reference,
/// or its `*_id` name claims another entity (the name-convention FK). A
/// `*_id` that resolves to nothing is a plain business value (an entity's own
/// natural-key part, #479) — only a reference we can SEE becomes a gap.
async fn classify_ref(
    db: &dyn GraphQuerier,
    prop: Option<&PropertyNode>,
    domain: &str,
    subject_title: &str,
    field_name: &str,
    dto_is_ref: bool,
) -> RefField {
    // The DTO-known reference arm: the graph edge first, then the shared
    // name-convention resolver, then the schema title the stem claims.
    if dto_is_ref {
        if let Some(prop) = prop {
            if let Some(title) = ref_target_title(db, prop).await {
                return RefField::Target(title);
            }
            if let Some(target) =
                resolve_convention_ref(db, prop, Some(domain), subject_title).await
            {
                return RefField::Target(target.title);
            }
        }
        return match resolve_stem_target(db, subject_title, field_name).await {
            Some(title) => RefField::Target(title),
            None => RefField::Unresolved,
        };
    }
    // Not a DTO reference: only the convention shape can still make it one,
    // and only when the name actually resolves to another entity (so a
    // natural-key part named `*_id` stays a plain value).
    if let Some(prop) = prop {
        if let Some(target) = resolve_convention_ref(db, prop, Some(domain), subject_title).await {
            return RefField::Target(target.title);
        }
        // A prop that exists but is not convention-shaped (optional, array,
        // already-suffixed, identity) can never be one.
        if prop.is_array || !prop.is_required || prop.is_id || prop.ref_target.is_some() {
            return RefField::No;
        }
    } else if !field_name.ends_with("_id") {
        return RefField::No;
    }
    match resolve_stem_target(db, subject_title, field_name).await {
        Some(title) => RefField::Target(title),
        None => RefField::No,
    }
}

/// Resolve a `*_id` field name's stem to an entity title (exact table/type
/// name match, subject excluded) — the last resort when no property or
/// `$ref` backs the column.
async fn resolve_stem_target(
    db: &dyn GraphQuerier,
    subject_title: &str,
    field_name: &str,
) -> Option<String> {
    let stem = field_name.strip_suffix("_id")?;
    let stem_norm = crate::fk_convention::normalize_convention_stem(stem);
    db.list_schemas(None).await.ok()?.into_iter().find_map(|s| {
        (s.is_entity
            && s.title != subject_title
            && (crate::fk_convention::normalize_convention_stem(&s.pg_table_name) == stem_norm
                || crate::fk_convention::normalize_convention_stem(&s.rust_type_name) == stem_norm))
            .then_some(s.title)
    })
}

/// Append preserving first position; a later duplicate replaces the value
/// (the required-entry seed wins over a DSL placeholder).
fn push_entry(entries: &mut Vec<(String, String)>, key: String, value: String) {
    match entries.iter_mut().find(|(existing, _)| existing == &key) {
        Some((_, existing_value)) => *existing_value = value,
        None => entries.push((key, value)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codegraph_core::types::SchemaNode;
    use codegraph_type_contracts::RefClassificationKind;

    fn schema(title: &str, table: &str) -> SchemaNode {
        SchemaNode {
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

    fn prop(name: &str, required: bool, rust_type: &str) -> PropertyNode {
        PropertyNode {
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
            pg_column_type: if rust_type.contains("Uuid") {
                "UUID".into()
            } else {
                "TEXT".into()
            },
            rust_field_name: codegraph_naming::to_snake_case(name),
            rust_field_type: rust_type.to_string(),
            sea_orm_type: rust_type.to_string(),
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

    fn identity(name: &str, rust_type: &str) -> PropertyNode {
        let mut p = prop(name, true, rust_type);
        p.is_id = true;
        p
    }

    fn ref_prop(name: &str, required: bool, target: &str) -> PropertyNode {
        let mut p = prop(name, required, "Uuid");
        p.ref_target = Some(format!("hr/json/{target}.json"));
        p.classification_kind = Some(RefClassificationKind::EntityReference);
        p
    }

    fn config(entities: &[&str]) -> DomainConfig {
        let list = entities
            .iter()
            .map(|e| format!("\"{e}\""))
            .collect::<Vec<_>>()
            .join(", ");
        toml::from_str(&format!(
            r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.hr]
label = "HR"
schema_dir = "hr"
postgres_schema = "hr"
entities = [{list}]
"#
        ))
        .unwrap()
    }

    fn create_field(name: &str, rust_type: &str) -> CreateField {
        CreateField {
            name: name.to_string(),
            rust_type: rust_type.to_string(),
            is_required: true,
            is_entity_ref: false,
            is_array: false,
        }
    }

    fn index(fields: &[CreateField]) -> HashMap<String, usize> {
        fields
            .iter()
            .enumerate()
            .map(|(i, f)| (f.name.clone(), i))
            .collect()
    }

    // ── The projection step (no graph) ───────────────────────────────────

    #[test]
    fn composite_wrappers_expand_to_their_columns_and_unknown_keys_drop() {
        let fields = vec![
            create_field("amount_cents", "i64"),
            create_field("amount_currency", "String"),
        ];
        let by_name = index(&fields);
        let composites = HashMap::from([(
            "amount".to_string(),
            vec![
                CompositeColumn {
                    name: "amount_cents".to_string(),
                    rust_type: "i64".to_string(),
                },
                CompositeColumn {
                    name: "amount_currency".to_string(),
                    rust_type: "String".to_string(),
                },
            ],
        )]);
        let entries = project_entries(
            vec![
                // The model spelling of the wrapper — no such DTO field.
                ("amount".to_string(), "'{ cents: 1 }'".to_string()),
                // A field the create DTO does not declare: `deny_unknown_fields`
                // would 422, so it never reaches the body.
                ("mystery".to_string(), "'Test mystery'".to_string()),
            ],
            &by_name,
            &fields,
            &[],
            &composites,
            &[RefField::No, RefField::No],
        );
        assert_eq!(
            entries,
            vec![
                ("amount_cents".to_string(), "4200".to_string()),
                ("amount_currency".to_string(), "'NZD'".to_string()),
            ]
        );
    }

    #[test]
    fn reference_placeholders_are_dropped_and_camel_case_projects() {
        let props = vec![ref_prop("vendor", true, "vendor")];
        let fields = vec![CreateField {
            name: "vendor_id".to_string(),
            rust_type: "uuid::Uuid".to_string(),
            is_required: true,
            is_entity_ref: true,
            is_array: false,
        }];
        let by_name = index(&fields);
        // The DSL may spell the reference either way; neither literal may ride
        // the wire — the required-ref pass seeds the parent and wires the id.
        let entries = project_entries(
            vec![
                ("vendorId".to_string(), "`uuid`".to_string()),
                ("vendor".to_string(), "'Test vendor'".to_string()),
            ],
            &by_name,
            &fields,
            &props,
            &HashMap::new(),
            &[RefField::Target("VendorType".to_string())],
        );
        assert!(entries.is_empty(), "{entries:?}");
    }

    #[test]
    fn a_codelist_model_spelling_projects_onto_the_stripped_dto_field() {
        let mut codelist = prop("workerTypeCode", true, "String");
        codelist.rust_field_name = "worker_type".to_string();
        codelist.ref_target = Some("codelist/WorkerTypeCodeList.json".to_string());
        codelist.classification_kind = Some(RefClassificationKind::CodelistReference);
        let props = vec![codelist];
        let fields = vec![create_field("worker_type", "String")];
        let by_name = index(&fields);
        let entries = project_entries(
            vec![("worker_type_code".to_string(), "'Permanent'".to_string())],
            &by_name,
            &fields,
            &props,
            &HashMap::new(),
            &[RefField::No],
        );
        assert_eq!(
            entries,
            vec![("worker_type".to_string(), "'Permanent'".to_string())]
        );
    }

    // ── The whole plan (MockEngine) ──────────────────────────────────────

    #[tokio::test]
    async fn identity_and_natural_key_members_are_seeded() {
        // Issue #806 / #522: a composite natural key's parts are REQUIRED
        // create-DTO members with no serde default — the pre-fix body omitted
        // them and the seed 422'd `missing field`.
        let engine = codegraph_core::mock::MockEngine::builder()
            .with_schema(schema("RuleType", "rule"))
            .with_properties(
                "RuleType",
                vec![
                    identity("code", "String"),
                    identity("ruleId", "uuid::Uuid"),
                    prop("label", true, "String"),
                ],
            )
            .build();
        let cfg = config(&["RuleType"]);
        let plan = plan_seed(
            &engine,
            &cfg,
            "v1",
            Some("RuleType"),
            Vec::new(),
            DtoKeyCasing::Snake,
        )
        .await
        .expect("DTO surface resolves");
        let by_name: HashMap<&str, &str> = plan
            .entries
            .iter()
            .map(|(f, v)| (f.as_str(), v.as_str()))
            .collect();
        assert_eq!(by_name.get("code"), Some(&"'Test code'"));
        assert!(
            by_name.get("rule_id").is_some_and(|v| v.starts_with('`')),
            "a uuid identity part seeds a valid uuid literal: {by_name:?}"
        );
        assert_eq!(by_name.get("label"), Some(&"'Test label'"));
        assert!(!by_name.contains_key("id"));
        assert!(plan.gaps.is_empty(), "{:?}", plan.gaps);
    }

    #[tokio::test]
    async fn a_convention_fk_seeds_its_parent_by_name() {
        // `case_id` carries no `$ref`/edge — the shared name-convention
        // resolver (issue #479/#806) finds CaseType, which is creatable.
        let engine = codegraph_core::mock::MockEngine::builder()
            .with_schema(schema("CaseType", "case"))
            .with_properties("CaseType", vec![prop("label", true, "String")])
            .with_schema(schema("ChildType", "child"))
            .with_properties(
                "ChildType",
                vec![
                    prop("title", true, "String"),
                    prop("caseId", true, "String"),
                ],
            )
            .build();
        let cfg = config(&["CaseType", "ChildType"]);
        let plan = plan_seed(
            &engine,
            &cfg,
            "v1",
            Some("ChildType"),
            vec![("title".to_string(), "'Filled'".to_string())],
            DtoKeyCasing::Snake,
        )
        .await
        .expect("DTO surface resolves");
        assert!(plan.gaps.is_empty(), "{:?}", plan.gaps);
        assert_eq!(plan.deps.len(), 1, "{:?}", plan.deps);
        assert_eq!(plan.deps[0].base_path, "/api/v1/hr/case");
        assert_eq!(
            plan.entries
                .iter()
                .find(|(f, _)| f == "case_id")
                .map(|(_, v)| v.as_str()),
            Some("dep_0_id")
        );
    }

    #[tokio::test]
    async fn a_dsl_fk_placeholder_is_replaced_by_the_parent_seed() {
        let engine = codegraph_core::mock::MockEngine::builder()
            .with_schema(schema("VendorType", "vendor"))
            .with_properties("VendorType", vec![prop("label", true, "String")])
            .with_schema(schema("OrderType", "order"))
            .with_properties(
                "OrderType",
                vec![
                    prop("title", true, "String"),
                    ref_prop("vendor", true, "vendor"),
                ],
            )
            .build();
        let cfg = config(&["VendorType", "OrderType"]);
        let plan = plan_seed(
            &engine,
            &cfg,
            "v1",
            Some("OrderType"),
            vec![(
                "vendorId".to_string(),
                "`00000000-0000-4000-8000-000000000000`".to_string(),
            )],
            DtoKeyCasing::Snake,
        )
        .await
        .expect("DTO surface resolves");
        assert!(plan.gaps.is_empty(), "{:?}", plan.gaps);
        assert_eq!(plan.deps.len(), 1);
        assert_eq!(
            plan.entries
                .iter()
                .find(|(f, _)| f == "vendor_id")
                .map(|(_, v)| v.as_str()),
            Some("dep_0_id"),
            "the random uuid must not ride the wire: {:?}",
            plan.entries
        );
    }

    #[tokio::test]
    async fn an_unresolvable_required_ref_is_a_named_gap_not_a_silent_omission() {
        let mut orphan_ref = ref_prop("orphan", true, "orphan");
        // The model declares the reference; no `Orphan` schema exists anywhere.
        orphan_ref.ref_target = Some("hr/json/orphan.json".to_string());
        let engine = codegraph_core::mock::MockEngine::builder()
            .with_schema(schema("ChildType", "child"))
            .with_properties("ChildType", vec![prop("title", true, "String"), orphan_ref])
            .build();
        let cfg = config(&["ChildType"]);
        let plan = plan_seed(
            &engine,
            &cfg,
            "v1",
            Some("ChildType"),
            Vec::new(),
            DtoKeyCasing::Snake,
        )
        .await
        .expect("DTO surface resolves");
        assert_eq!(plan.gaps.len(), 1, "{:?}", plan.gaps);
        assert_eq!(plan.gaps[0].entity, "ChildType");
        assert_eq!(plan.gaps[0].field, "orphan_id");
        assert!(
            plan.gaps[0].reason.contains("no resolvable entity target"),
            "{}",
            plan.gaps[0].reason
        );
        assert!(
            !plan.entries.iter().any(|(f, _)| f == "orphan_id"),
            "a field we cannot seed is never posted as a random uuid: {:?}",
            plan.entries
        );
    }

    #[tokio::test]
    async fn an_optional_reference_is_left_null() {
        let engine = codegraph_core::mock::MockEngine::builder()
            .with_schema(schema("VendorType", "vendor"))
            .with_properties("VendorType", vec![prop("label", true, "String")])
            .with_schema(schema("OrderType", "order"))
            .with_properties(
                "OrderType",
                vec![
                    prop("title", true, "String"),
                    ref_prop("vendor", false, "vendor"),
                ],
            )
            .build();
        let cfg = config(&["VendorType", "OrderType"]);
        let plan = plan_seed(
            &engine,
            &cfg,
            "v1",
            Some("OrderType"),
            vec![("vendorId".to_string(), "`uuid`".to_string())],
            DtoKeyCasing::Snake,
        )
        .await
        .expect("DTO surface resolves");
        assert!(plan.deps.is_empty());
        assert!(!plan.entries.iter().any(|(f, _)| f == "vendor_id"));
        assert!(plan.gaps.is_empty());
    }

    #[tokio::test]
    async fn no_owning_domain_means_no_dto_surface() {
        let mut orphan = schema("OrphanType", "orphan");
        orphan.domain = None;
        let engine = codegraph_core::mock::MockEngine::builder()
            .with_schema(orphan)
            .with_properties("OrphanType", vec![prop("label", true, "String")])
            .build();
        let cfg = config(&["OrphanType"]);
        assert!(
            plan_seed(
                &engine,
                &cfg,
                "v1",
                Some("OrphanType"),
                Vec::new(),
                DtoKeyCasing::Snake
            )
            .await
            .is_none(),
            "the caller must fall back to the prop-driven path"
        );
    }
}
