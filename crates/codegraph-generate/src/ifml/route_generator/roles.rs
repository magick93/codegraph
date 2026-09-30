use codegraph_config::{IfmlComponentMapping, SemanticRole};
use rex_ifml::ComponentSpec;

use crate::ifml::context::{IfmlComponent, IfmlViewContainer};
use crate::ifml::control_core;

use super::context::{RenderChart, RenderForm, RenderMapping, RenderTable};

/// The layout kind used for mapping resolution: the typed spec kind when a
/// spec is present, else the component type.
pub(super) fn kind_of(c: &IfmlComponent) -> String {
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
pub(super) fn component_role(c: &IfmlComponent) -> Option<SemanticRole> {
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
pub(super) fn input_field_role(input_type: &str) -> Option<SemanticRole> {
    control_core::input_field_role(input_type)
}

/// Event/button slot role: submit-style and click events drive actions.
pub(super) fn event_role(event_type: &str) -> Option<SemanticRole> {
    match event_type {
        "save" | "submit" | "cancel" | "back" | "click" => Some(SemanticRole::ActionControl),
        _ => None,
    }
}

/// View slot role: modals render as `modal-view`, landmarks are `shell`
/// candidates.
pub(super) fn semantic_view_role(vc: &IfmlViewContainer) -> Option<SemanticRole> {
    if vc.is_modal {
        Some(SemanticRole::ModalView)
    } else if vc.is_landmark {
        Some(SemanticRole::Shell)
    } else {
        None
    }
}

/// Container slot role: xor/wizard groupings are presentation containers.
pub(super) fn semantic_container_role(vc: &IfmlViewContainer) -> Option<SemanticRole> {
    if vc.is_xor {
        Some(SemanticRole::PresentationContainer)
    } else {
        None
    }
}

pub(super) fn is_collection(c: &IfmlComponent) -> bool {
    matches!(c.spec, Some(ComponentSpec::Table(_))) || c.component_type == "list"
}

pub(super) fn is_form_component(c: &IfmlComponent) -> bool {
    matches!(c.spec, Some(ComponentSpec::Form(_))) || c.component_type == "form"
}

pub(super) fn mapped_fields(
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

pub(super) fn mapping_context(m: &IfmlComponentMapping) -> RenderMapping {
    RenderMapping {
        import_name: m.export_name().to_string(),
        import_path: m.path.clone(),
        testid: m.testid("root").map(str::to_string),
        row_testid: m.testid("row").map(str::to_string),
    }
}
