use std::collections::{BTreeMap, HashMap, HashSet};

use codegraph_config::ux::UxRules;
use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::PropertyNode;
use codegraph_type_contracts::RefClassificationKind;
use rex_ifml::{ColumnDef, ComponentSpec};

use crate::error::Result;
use crate::ifml::context::IfmlComponent;

use super::context::{PageUxContext, RenderColumn, RenderColumnUx, RenderTable, RenderTimeline};
use super::roles::is_collection;
use super::workflow::{js_quote, workflow_for_entity};

/// Resolve the graph schema title for an IFML entity binding: exact match
/// first, then the `{entity}Type` shape (mirrors the IFML querier's
/// `resolve_schema_title_by_name`). `None` when the entity is not
/// schema-backed — ux columns then resolve from the component's
/// `(field, rust_type)` pairs alone.
async fn ux_schema_title(db: &dyn GraphQuerier, entity: &str) -> Option<String> {
    if entity.is_empty() {
        return None;
    }
    if matches!(db.get_schema(entity).await, Ok(Some(_))) {
        return Some(entity.to_string());
    }
    let suffixed = format!("{entity}Type");
    matches!(db.get_schema(&suffixed).await, Ok(Some(_))).then_some(suffixed)
}

/// Graph properties for an IFML component's bound entity, keyed by both
/// the property name and the `r#`-stripped rust field name. Empty when the
/// entity is not schema-backed (columns fall back to the component's
/// `(field, rust_type)` pairs — an honest, signal-poor inference).
pub(super) async fn ux_props_for_entity(
    db: &dyn GraphQuerier,
    entity: &str,
) -> HashMap<String, PropertyNode> {
    let Some(title) = ux_schema_title(db, entity).await else {
        return HashMap::new();
    };
    let Ok(props) = db.get_properties(&title).await else {
        return HashMap::new();
    };
    let mut by_name: HashMap<String, PropertyNode> = HashMap::with_capacity(props.len() * 2);
    for prop in props {
        let stripped = prop
            .rust_field_name
            .strip_prefix("r#")
            .unwrap_or(&prop.rust_field_name)
            .to_string();
        by_name.insert(prop.name.clone(), prop.clone());
        by_name.insert(stripped, prop);
    }
    by_name
}

/// The codelist half of the `ifml_control_inference.rs` mirror: graph
/// classification kinds whose value sets live in the graph. (InlineEnum
/// properties are detected through their projected `select` input type
/// instead — the same signal Pass 1 reads.)
fn column_is_codelist(prop: &PropertyNode) -> bool {
    matches!(
        prop.effective_kind(),
        Some(RefClassificationKind::CodelistReference) | Some(RefClassificationKind::CodelistCheck)
    )
}

/// The entity-ref half of the `ifml_control_inference.rs` mirror.
fn column_is_entity_ref(prop: &PropertyNode) -> bool {
    prop.effective_kind() == Some(RefClassificationKind::EntityReference)
}

/// A signal-poor [`UiField`] for columns whose property is absent from the
/// graph: the component's `(field, rust_type)` pair is projected through
/// the entity pipeline's rust→ts mapping. No pg type exists here, so the
/// pg-backed branches of Pass 1 (money/quantity/time-point) honestly
/// cannot fire.
fn ux_synth_field(
    binding: &str,
    fields_with_types: &[(String, String)],
) -> crate::ui::page::UiField {
    let rust_type = fields_with_types
        .iter()
        .find(|(name, _)| name == binding)
        .map(|(_, t)| t.as_str())
        .unwrap_or("String");
    crate::ui::page::UiField {
        name: binding.to_string(),
        label: String::new(),
        ts_type: crate::ui::form::rust_type_to_ts(rust_type, false),
        input_type: String::new(),
        is_required: false,
        is_array: rust_type.starts_with("Vec<"),
        is_entity_ref: false,
        is_immutable: false,
        is_codelist: false,
        is_range: false,
        codelist_values: Vec::new(),
        description: String::new(),
        pg_type: String::new(),
        open_end: false,
        ref_api_path: None,
        structured_sub_fields: Vec::new(),
        nested_type_name: None,
    }
}

/// ux-rules resolution for ONE fallback-table column (issue #300).
///
/// Precedence, highest first (the future DSL `dimension:` key slots above
/// rules; then rules > pack defaults > inference):
///
/// 1. `kind == "lookup"` — the DSL names the presentation explicitly
///    (`via status_labels`): StatusCategory + chip with the pack tone map
///    (see [`RenderColumnUx::for_lookup`]). No rule can downgrade an
///    explicit lookup.
/// 2. Everything else resolves through the shared Pass-1 + rule tier
///    ([`crate::ux::plan::resolve_column`] folded by `into_column_plan`):
///    Pass 1 infers from the bound property's GRAPH metadata — the entity
///    pipeline's exact projection ([`crate::ui::form::ui_field_from_property`])
///    over classification kind, pg type, and input type — then the
///    first-match `[[column]]` rule (project rules ahead of pack rules)
///    overrides the inference and pins display/align/tone/sortable
///    payloads.
/// 3. `kind == "expr"` infers from what the model actually has — nothing:
///    the IFML AST carries no expression return type, so the bound
///    property (if any) is NOT consulted and Pass 1 runs signal-poor over
///    the rendered name (no inference theater). Rules may still pin
///    display/align payloads on the column's name.
///
/// When the bound property is absent from the graph (schema-less IFML
/// runs), a signal-poor [`UiField`] is synthesized from the component's
/// `(field, rust_type)` pairs ([`ux_synth_field`]).
pub(super) fn resolve_column_ux(
    rules: &UxRules,
    kind: &str,
    binding: &str,
    props: &HashMap<String, PropertyNode>,
    fields_with_types: &[(String, String)],
    workflow_status_field: Option<&str>,
) -> RenderColumnUx {
    if kind == "lookup" {
        return RenderColumnUx::for_lookup();
    }
    let prop = if kind == "expr" {
        None
    } else {
        props.get(binding)
    };
    let field = match prop {
        Some(prop) => crate::ui::form::ui_field_from_property(
            prop,
            column_is_entity_ref(prop),
            column_is_codelist(prop),
            &[],
            &[],
            &prop.pg_column_type,
            prop.pg_column_type.contains("RANGE"),
            false,
        ),
        None => ux_synth_field(binding, fields_with_types),
    };
    let resolution = crate::ux::plan::resolve_column(rules, prop, &field, workflow_status_field);
    RenderColumnUx::from_plan(&resolution.into_column_plan(rules.format.clone()))
}

/// Apply the ux resolution to every column of a typed fallback table.
pub(super) fn apply_table_ux(
    table: &mut RenderTable,
    rules: &UxRules,
    props: &HashMap<String, PropertyNode>,
    fields_with_types: &[(String, String)],
    workflow_status_field: Option<&str>,
) {
    for col in &mut table.columns {
        col.ux = Some(resolve_column_ux(
            rules,
            &col.kind,
            &col.binding,
            props,
            fields_with_types,
            workflow_status_field,
        ));
    }
}

/// Generation-scoped ux-rules context (issue #301): the resolved rules
/// plus one plan per distinct bound entity, built ONCE per generation by
/// [`resolve_generation_ux`]. Page contexts reuse the plan output verbatim
/// (timeline layout) instead of re-resolving — no second validation path.
pub(crate) struct UxGeneration<'a> {
    /// The project's resolved rules (`project.ux`).
    pub rules: &'a UxRules,
    /// Bound entity name → built plan. Flag-off generations carry an
    /// empty map.
    pub plans: &'a HashMap<String, crate::ux::plan::UxPlan>,
}

/// Resolve the ux-rules plane for a whole IFML generation (issue #301).
///
/// One plan per distinct bound entity over its collection components'
/// display fields — the same projection #300 uses for columns: the graph
/// property when the entity is schema-backed, a synthesized
/// `(field, rust_type)` pair otherwise. Advisory diagnostics are collected
/// over each plan and returned as DEDUPLICATED report lines for the
/// caller's stderr warning channel (mirroring the entity pipeline's
/// ui-page generator). Flag off ⇒ an empty map and no lines.
///
/// Errors when an explicit timeline rule's `order_by` does not resolve to
/// one of the entity's TimePoint fields — the error names the candidates
/// (a hard generation error, surfacing through the global phase's `?`).
pub(super) async fn resolve_generation_ux(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    model: &crate::ifml::context::IfmlModel,
    rules: Option<&UxRules>,
) -> Result<(HashMap<String, crate::ux::plan::UxPlan>, Vec<String>)> {
    let Some(rules) = rules else {
        return Ok((HashMap::new(), Vec::new()));
    };
    let mut plans: HashMap<String, crate::ux::plan::UxPlan> = HashMap::new();
    let mut reported: HashSet<String> = HashSet::new();
    let mut lines: Vec<String> = Vec::new();
    for vc in &model.view_containers {
        for c in vc
            .components
            .iter()
            .chain(vc.containers.iter().flat_map(|g| g.components.iter()))
        {
            if !is_collection(c) {
                continue;
            }
            let Some(entity) = c.entity.as_deref().filter(|e| !e.is_empty()) else {
                continue;
            };
            // Resolve once per entity, not per component: the first
            // collection bound to the entity fixes its plan for the run.
            if plans.contains_key(entity) {
                continue;
            }
            let props = ux_props_for_entity(db, entity).await;
            let status_field = workflow_for_entity(config, entity).map(|wf| wf.status_field);
            let entity_input = ux_entity_input(c, &props, status_field.as_deref());
            let input = entity_input.plan_input(entity);
            if let Some(plan) = crate::ux::plan::build_ux_plan(Some(rules), &input)? {
                let diag = crate::ux::diagnostics::collect_diagnostics(rules, &input, &plan);
                for line in crate::ux::diagnostics::report(&diag) {
                    if reported.insert(line.clone()) {
                        lines.push(line);
                    }
                }
                plans.insert(entity.to_string(), plan);
            }
        }
    }
    Ok((plans, lines))
}

/// The plan-input display fields of a collection component: the declared
/// `fields` when present, else the typed-table column bindings
/// (expression columns carry no property and are skipped).
fn ux_plan_fields(c: &IfmlComponent) -> Vec<String> {
    if !c.fields.is_empty() {
        return c.fields.clone();
    }
    match &c.spec {
        Some(ComponentSpec::Table(spec)) => spec
            .columns
            .iter()
            .filter_map(|col| match col {
                ColumnDef::Field { field, .. } | ColumnDef::Lookup { field, .. } => {
                    Some(field.property.clone())
                }
                ColumnDef::Expression { .. } => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Owned plan-input holder for one collection component ([`UxPlanInput`]
/// borrows, so the owner must outlive the plan build — see
/// [`UxEntityInput::plan_input`]).
struct UxEntityInput<'a> {
    fields: Vec<crate::ui::page::UiField>,
    prop_by_name: BTreeMap<String, &'a PropertyNode>,
    workflow_status_field: Option<&'a str>,
}

impl UxEntityInput<'_> {
    /// Borrow into the plan input [`crate::ux::plan::build_ux_plan`]
    /// consumes.
    fn plan_input<'b>(&'b self, entity: &'b str) -> crate::ux::plan::UxPlanInput<'b> {
        crate::ux::plan::UxPlanInput {
            entity_title: entity,
            fields: &self.fields,
            prop_by_name: self
                .prop_by_name
                .iter()
                .map(|(name, prop)| (name.as_str(), *prop))
                .collect(),
            workflow_status_field: self.workflow_status_field,
            workflow_terminal_states: &[],
            has_soft_delete: false,
            user_pinned_list_order: false,
        }
    }
}

/// The plan-input projection of one collection component. Schema-backed
/// entities plan over ALL their properties — the timeline `order_by` must
/// resolve to a TimePoint property of the bound entity, whether or not a
/// component displays it — with names sorted for deterministic output.
/// Schema-less projections fall back to the component's display fields
/// ([`ux_plan_fields`], signal-poor synthesized [`UiField`] pairs).
fn ux_entity_input<'a>(
    c: &IfmlComponent,
    props: &'a HashMap<String, PropertyNode>,
    workflow_status_field: Option<&'a str>,
) -> UxEntityInput<'a> {
    let names: Vec<String> = if props.is_empty() {
        ux_plan_fields(c)
    } else {
        let mut names: Vec<String> = props
            .iter()
            .filter(|(key, prop)| prop.name == **key)
            .map(|(key, _)| key.clone())
            .collect();
        names.sort();
        names
    };
    let mut fields: Vec<crate::ui::page::UiField> = Vec::with_capacity(names.len());
    let mut prop_by_name: BTreeMap<String, &'a PropertyNode> = BTreeMap::new();
    for name in &names {
        if let Some(prop) = props.get(name) {
            prop_by_name.insert(name.clone(), prop);
            fields.push(crate::ui::form::ui_field_from_property(
                prop,
                column_is_entity_ref(prop),
                column_is_codelist(prop),
                &[],
                &[],
                &prop.pg_column_type,
                prop.pg_column_type.contains("RANGE"),
                false,
            ));
        } else {
            fields.push(ux_synth_field(name, &c.fields_with_types));
        }
    }
    UxEntityInput {
        fields,
        prop_by_name,
        workflow_status_field,
    }
}

/// Project the entity plan's collection shape onto the component (issue
/// #301): a Timeline plan yields the render payload, with preview columns
/// resolved through the SAME per-column ux tier as #300 cells. Table
/// plans (and entities without a plan) yield `None`. The plan output is
/// reused verbatim — never re-validated.
pub(super) fn timeline_from_plan(
    uxg: &UxGeneration<'_>,
    entity: &str,
    rules: &UxRules,
    props: &HashMap<String, PropertyNode>,
    fields_with_types: &[(String, String)],
    workflow_status_field: Option<&str>,
) -> Option<RenderTimeline> {
    let plan = uxg.plans.get(entity)?;
    match &plan.collection {
        crate::ux::plan::CollectionPlan::Timeline {
            order_by,
            title_field,
            preview,
        } => Some(RenderTimeline {
            order_binding: order_by.clone(),
            title_binding: title_field.clone(),
            preview: preview
                .iter()
                .map(|name| {
                    preview_column(rules, name, props, fields_with_types, workflow_status_field)
                })
                .collect(),
        }),
        crate::ux::plan::CollectionPlan::Table => None,
    }
}

/// One timeline preview column: the shared #300 column resolution over the
/// field's graph property (or its synthesized `(field, rust_type)` pair).
fn preview_column(
    rules: &UxRules,
    name: &str,
    props: &HashMap<String, PropertyNode>,
    fields_with_types: &[(String, String)],
    workflow_status_field: Option<&str>,
) -> RenderColumn {
    RenderColumn {
        label: name.to_string(),
        kind: "field".to_string(),
        binding: name.to_string(),
        lookup: String::new(),
        expr: String::new(),
        ux: Some(resolve_column_ux(
            rules,
            "field",
            name,
            props,
            fields_with_types,
            workflow_status_field,
        )),
    }
}

/// The page-level ux formatting context: locale plus a ready-to-render
/// money options object literal (`Intl.NumberFormat`). Currency-less packs
/// degrade money cells to grouped decimals (`{}` options).
pub(super) fn page_ux_context(rules: &UxRules) -> PageUxContext {
    PageUxContext {
        locale: rules.format.locale.clone(),
        money_options: match rules.format.currency.as_deref() {
            Some(code) => format!("{{ style: 'currency', currency: {} }}", js_quote(code)),
            None => "{}".to_string(),
        },
    }
}
