//! Shared IFML component → selector resolution.
//!
//! [`ComponentSelectors::for_component`] is the single source of the
//! name → type → role → kind mapping-tier walk (with the
//! `{component}-{kind}` fallback): the IFML e2e generator resolves the
//! testids its specs assert through it, and the POM generator (#316/#317)
//! consumes the same function — mapped-component testids then resolve
//! identically to markup by construction.
//!
//! The fallback markup contract lives in the IFML templates
//! (`templates/ifml/svelte/page.tera`): `{component}-table`,
//! `{component}-row`, `{component}-form`, `{component}-submit`,
//! `{component}-details`. The full testid table (entity + IFML families)
//! is documented in [`crate::ux::plan::ids`].

use codegraph_config::{IfmlComponentMappings, SemanticRole};
use rex_ifml::ComponentSpec;

use super::context::IfmlComponent;

/// Resolved testids for one IFML component, as the e2e/POM generators read
/// them: `root` is the component's primary selector (render assertion),
/// `row`/`form`/`submit` drive click-through and CRUD interactions. A
/// `None` field means the component kind carries no such slot (or the
/// mapped component declared no testid for it).
#[derive(Debug, Default)]
pub struct ComponentSelectors {
    pub(crate) root: Option<String>,
    pub(crate) row: Option<String>,
    pub(crate) form: Option<String>,
    pub(crate) submit: Option<String>,
}

impl ComponentSelectors {
    /// Resolve the selectors for one component of view `view_name`.
    ///
    /// Tier order (first match wins, view-scoped entries only match inside
    /// their view): component name → component type → kind, per
    /// `IfmlComponentMappings::resolve`. A mapped component's `root`/`form`
    /// testids both count as the root selector; the form branch additionally
    /// probes an `action-control`-roled slot for the submit testid. Without
    /// a mapping the built-in fallback markup testids apply.
    pub fn for_component(
        mappings: Option<&IfmlComponentMappings>,
        view_name: &str,
        c: &IfmlComponent,
    ) -> Self {
        let kind = component_kind(c);
        if let Some(mapped) =
            mappings.and_then(|m| m.resolve(view_name, &c.name, &c.component_type, &kind))
        {
            return ComponentSelectors {
                root: mapped
                    .testid("root")
                    .or_else(|| mapped.testid("form"))
                    .map(str::to_string),
                row: mapped.testid("row").map(str::to_string),
                form: mapped.testid("form").map(str::to_string),
                submit: mapped.testid("submit").map(str::to_string),
            };
        }
        if is_collection(c) {
            ComponentSelectors {
                root: Some(format!("{}-table", c.name)),
                row: Some(format!("{}-row", c.name)),
                form: None,
                submit: None,
            }
        } else if is_form(c) {
            let submit = mappings
                .and_then(|m| {
                    m.resolve_slot(
                        view_name,
                        &c.name,
                        &c.component_type,
                        &kind,
                        Some(SemanticRole::ActionControl),
                    )
                })
                .and_then(|m| m.testid("root").map(str::to_string));
            ComponentSelectors {
                root: Some(format!("{}-form", c.name)),
                row: None,
                form: Some(format!("{}-form", c.name)),
                submit: Some(submit.unwrap_or_else(|| format!("{}-submit", c.name))),
            }
        } else if is_details(c) {
            ComponentSelectors {
                root: Some(format!("{}-details", c.name)),
                row: None,
                form: None,
                submit: None,
            }
        } else {
            ComponentSelectors::default()
        }
    }
}

/// The mappings `kind` of a component: the typed spec kind when present,
/// else the declared component type.
pub(crate) fn component_kind(c: &IfmlComponent) -> String {
    match c.spec {
        Some(ComponentSpec::Table(_)) => "table".to_string(),
        Some(ComponentSpec::Form(_)) => "form".to_string(),
        Some(ComponentSpec::Chart(_)) => "chart".to_string(),
        None => c.component_type.clone(),
    }
}

/// A list/table component (typed table spec, or a list/table type).
pub(crate) fn is_collection(c: &IfmlComponent) -> bool {
    matches!(c.spec, Some(ComponentSpec::Table(_)))
        || c.component_type == "list"
        || c.component_type == "table"
}

/// A form component (typed form spec, or a form type).
pub(crate) fn is_form(c: &IfmlComponent) -> bool {
    matches!(c.spec, Some(ComponentSpec::Form(_))) || c.component_type == "form"
}

/// A details component.
pub(crate) fn is_details(c: &IfmlComponent) -> bool {
    c.component_type == "details"
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use rex_ifml::TableSpec;

    fn component(name: &str, component_type: &str) -> IfmlComponent {
        IfmlComponent {
            name: name.to_string(),
            component_type: component_type.to_string(),
            mode: None,
            entity: None,
            fields: Vec::new(),
            fields_with_types: Vec::new(),
            filter: None,
            properties: HashMap::new(),
            events: Vec::new(),
            parts: Vec::new(),
            spec: None,
        }
    }

    #[test]
    fn fallback_testids_follow_component_kind() {
        let table = component("grid", "list");
        let s = ComponentSelectors::for_component(None, "CustomerList", &table);
        assert_eq!(s.root.as_deref(), Some("grid-table"));
        assert_eq!(s.row.as_deref(), Some("grid-row"));
        assert!(s.form.is_none() && s.submit.is_none());

        let form = component("editor", "form");
        let s = ComponentSelectors::for_component(None, "CustomerForm", &form);
        assert_eq!(s.root.as_deref(), Some("editor-form"));
        assert_eq!(s.form.as_deref(), Some("editor-form"));
        assert_eq!(s.submit.as_deref(), Some("editor-submit"));

        let details = component("summary", "details");
        let s = ComponentSelectors::for_component(None, "CustomerDetail", &details);
        assert_eq!(s.root.as_deref(), Some("summary-details"));

        let other = component("spark", "chart");
        let s = ComponentSelectors::for_component(None, "Dashboard", &other);
        assert!(s.root.is_none() && s.row.is_none() && s.form.is_none() && s.submit.is_none());
    }

    #[test]
    fn typed_spec_overrides_declared_type() {
        // A typed table spec wins over a "form" component_type for both the
        // kind used in mapping lookups and the fallback branch.
        let mut typed_table = component("editor", "form");
        typed_table.spec = Some(ComponentSpec::Table(TableSpec {
            columns: Vec::new(),
            pagination: false,
        }));
        let s = ComponentSelectors::for_component(None, "CustomerList", &typed_table);
        assert_eq!(s.root.as_deref(), Some("editor-table"));
        assert_eq!(s.row.as_deref(), Some("editor-row"));
        assert!(s.form.is_none() && s.submit.is_none());
    }

    #[test]
    fn mapped_testids_win_and_role_slot_supplies_submit() {
        let mappings = IfmlComponentMappings::parse_str(
            r#"
[[component]]
kind = "table"
path = "$lib/components/DataTable.svelte"
export = "DataTable"
testids = { root = "data-table", row = "data-row" }

[[component]]
role = "action-control"
path = "$lib/components/Button.svelte"
export = "Button"
testids = { root = "save-button" }

[[component]]
kind = "form"
path = "$lib/components/Form.svelte"
export = "Form"
testids = { root = "mapped-form", form = "mapped-form", submit = "mapped-submit" }
"#,
        )
        .unwrap();

        // component_type "table" so the kind tier matches the table mapping.
        let table = component("grid", "table");
        let s = ComponentSelectors::for_component(Some(&mappings), "CustomerList", &table);
        assert_eq!(s.root.as_deref(), Some("data-table"));
        assert_eq!(s.row.as_deref(), Some("data-row"));

        // Mapped form: submit testid comes from the mapping, not the
        // action-control slot.
        let form = component("editor", "form");
        let s = ComponentSelectors::for_component(Some(&mappings), "CustomerForm", &form);
        assert_eq!(s.root.as_deref(), Some("mapped-form"));
        assert_eq!(s.submit.as_deref(), Some("mapped-submit"));

        // Unmapped form falls back, but the action-control slot still
        // supplies the submit testid.
        let mappings_no_form = IfmlComponentMappings::parse_str(
            r#"
[[component]]
role = "action-control"
path = "$lib/components/Button.svelte"
export = "Button"
testids = { root = "save-button" }
"#,
        )
        .unwrap();
        let s = ComponentSelectors::for_component(Some(&mappings_no_form), "CustomerForm", &form);
        assert_eq!(s.root.as_deref(), Some("editor-form"));
        assert_eq!(s.submit.as_deref(), Some("save-button"));

        // View-scoped entries do not match another view.
        let mappings_view = IfmlComponentMappings::parse_str(
            r#"
[[component]]
name = "grid"
view = "OtherList"
path = "$lib/components/X.svelte"
testids = { root = "x" }
"#,
        )
        .unwrap();
        let s = ComponentSelectors::for_component(Some(&mappings_view), "CustomerList", &table);
        assert_eq!(
            s.root.as_deref(),
            Some("grid-table"),
            "view mismatch falls back"
        );
    }
}
