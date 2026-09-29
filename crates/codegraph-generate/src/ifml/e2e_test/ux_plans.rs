//! ux-rules plan plumbing for the IFML e2e generator (issue #303, #317
//! module split): one [`UxPlan`] per distinct bound entity, built through
//! the SAME projection the route generator's `resolve_generation_ux` uses,
//! minus the advisory diagnostics the e2e generator never prints.

use std::collections::{BTreeMap, HashMap};

use codegraph_config::ux::UxRules;
use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::PropertyNode;
use codegraph_type_contracts::RefClassificationKind;
use rex_ifml::ComponentSpec;

use crate::error::Result;
use crate::ux::plan::{build_ux_plan, UxPlan, UxPlanInput};

use super::super::context::{IfmlComponent, IfmlModel};
use super::super::route_generator::workflow_for_entity;
use super::super::selectors::is_collection;

/// One ux plan per distinct bound entity (issue #303) — the same projection
/// the route generator's `resolve_generation_ux` builds (schema-backed
/// entities plan over ALL their properties, schema-less ones over the
/// component's display fields).
pub(crate) async fn build_ux_plans(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    model: &IfmlModel,
    rules: &UxRules,
) -> Result<HashMap<String, UxPlan>> {
    let mut plans: HashMap<String, UxPlan> = HashMap::new();
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
            if plans.contains_key(entity) {
                continue;
            }
            let props = ux_props_for_entity(db, entity).await;
            let status_field = workflow_for_entity(config, entity).map(|wf| wf.status_field);
            let status_field = status_field.as_deref();
            let names = ux_plan_field_names(c, &props);
            let mut fields: Vec<crate::ui::page::UiField> = Vec::with_capacity(names.len());
            let mut prop_by_name: BTreeMap<String, &PropertyNode> = BTreeMap::new();
            for name in &names {
                if let Some(prop) = props.get(name) {
                    prop_by_name.insert(name.clone(), prop);
                    fields.push(ux_ui_field(prop));
                } else {
                    fields.push(ux_synth_field(name, &c.fields_with_types));
                }
            }
            let input = UxPlanInput {
                entity_title: entity,
                fields: &fields,
                prop_by_name: prop_by_name
                    .iter()
                    .map(|(name, prop)| (name.as_str(), *prop))
                    .collect(),
                workflow_status_field: status_field,
                workflow_terminal_states: &[],
                has_soft_delete: false,
                user_pinned_list_order: false,
            };
            if let Some(plan) = build_ux_plan(Some(rules), &input)? {
                plans.insert(entity.to_string(), plan);
            }
        }
    }
    Ok(plans)
}

/// The plan-input display field names of a collection component: the
/// declared `fields` when present, else the typed-table column bindings
/// (expression columns carry no property and are skipped).
fn ux_plan_field_names(c: &IfmlComponent, props: &HashMap<String, PropertyNode>) -> Vec<String> {
    if !props.is_empty() {
        let mut names: Vec<String> = props
            .iter()
            .filter(|(key, prop)| prop.name == **key)
            .map(|(key, _)| key.clone())
            .collect();
        names.sort();
        return names;
    }
    if !c.fields.is_empty() {
        return c.fields.clone();
    }
    match &c.spec {
        Some(ComponentSpec::Table(spec)) => spec
            .columns
            .iter()
            .filter_map(|col| match col {
                rex_ifml::ColumnDef::Field { field, .. }
                | rex_ifml::ColumnDef::Lookup { field, .. } => Some(field.property.clone()),
                rex_ifml::ColumnDef::Expression { .. } => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// The graph properties of an IFML component's bound entity keyed by
/// property name (mirroring the route generator's resolution).
async fn ux_props_for_entity(db: &dyn GraphQuerier, entity: &str) -> HashMap<String, PropertyNode> {
    let Some(title) = ux_schema_title(db, entity).await else {
        return HashMap::new();
    };
    let Ok(props) = db.get_properties(&title).await else {
        return HashMap::new();
    };
    props
        .into_iter()
        .map(|prop| (prop.name.clone(), prop))
        .collect()
}

/// Resolve the graph schema title for an IFML entity binding: exact match
/// first, then the `{entity}Type` shape.
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

/// The entity pipeline's UI-field projection of one graph property.
pub(crate) fn ux_ui_field(prop: &PropertyNode) -> crate::ui::page::UiField {
    crate::ui::form::ui_field_from_property(
        prop,
        column_is_entity_ref(prop),
        column_is_codelist(prop),
        &[],
        &[],
        &prop.pg_column_type,
        prop.pg_column_type.contains("RANGE"),
        false,
    )
}

pub(crate) fn column_is_codelist(prop: &PropertyNode) -> bool {
    matches!(
        prop.effective_kind(),
        Some(RefClassificationKind::CodelistReference) | Some(RefClassificationKind::CodelistCheck)
    )
}

fn column_is_entity_ref(prop: &PropertyNode) -> bool {
    prop.effective_kind() == Some(RefClassificationKind::EntityReference)
}

/// A signal-poor [`crate::ui::page::UiField`] for columns whose property is
/// absent from the graph.
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
