//! Page heading ownership (issue #808): does a generated IFML page render
//! the view `<h1>`, and which component carries it?
//!
//! The page templates (`templates/ifml/svelte/page{,_tanstack}.tera`) emit
//! `<h1>{{ label }}</h1>` in exactly four branches: the mapped-component
//! branch (`comp.mapping`), the timeline branch (`comp.timeline`), the typed
//! table branch (`comp.table`) and the fallback list branch
//! (`component_type == "list"`). The `form`, `details` and `chart` branches
//! render no `<h1>` — so a page whose only components are those renders no
//! heading at all, and the e2e plane must not assert one.
//!
//! Both consumers read this ONE predicate: the route generator (which
//! decides which component's `show_heading` is set — exactly one `<h1>` per
//! page, on the first component whose markup can carry it) and the e2e
//! generator (render/persona `assert_heading`, journey arrival and the POM
//! `heading()` fallback). A heading-less page keeps its primary root as the
//! asserted surface instead.

use codegraph_config::{IfmlComponentMapping, IfmlComponentMappings};
use phero_ifml::ComponentSpec;

use super::context::{IfmlComponent, IfmlViewContainer};
use super::route_generator::roles::{component_role, kind_of};

/// The whole-component mapping the route generator renders a component
/// through — the mapped-component template branch wins exactly when this
/// resolves.
///
/// This is the SAME walk the render context performs
/// (`IfmlComponentMappings::resolve_slot`, name → type → role → kind) with
/// the SAME kind/role inputs, so "mapped" can never drift between the
/// markup and this predicate (#808).
pub(crate) fn component_mapping<'a>(
    view_name: &str,
    c: &IfmlComponent,
    mappings: Option<&'a IfmlComponentMappings>,
) -> Option<&'a IfmlComponentMapping> {
    let kind = kind_of(c);
    let role = component_role(c);
    mappings?.resolve_slot(view_name, &c.name, &c.component_type, &kind, role)
}

/// Whether this component's markup can carry the page `<h1>`: the
/// mapped-component branch, a typed table payload (a timeline layout rides
/// the table spec, so it is covered by the same arm), or a fallback
/// `type: list` collection.
///
/// A specless `type: table` component renders NOTHING (`comp.table` comes
/// from the typed spec — pinned by
/// `route_generator::tests::template_specless_table_component_renders_nothing`),
/// so it is deliberately not heading-capable here even though the selector
/// plane treats it as a collection.
pub(crate) fn component_renders_heading(
    view_name: &str,
    c: &IfmlComponent,
    mappings: Option<&IfmlComponentMappings>,
) -> bool {
    component_mapping(view_name, c, mappings).is_some()
        || matches!(c.spec, Some(ComponentSpec::Table(_)))
        || c.component_type == "list"
}

/// The name of the FIRST top-level component that can render the page
/// `<h1>`, else `None`. Container-nested components never own the page
/// heading (`show_heading` is suppressed inside presentation wrappers).
pub(crate) fn page_heading_owner(
    vc: &IfmlViewContainer,
    mappings: Option<&IfmlComponentMappings>,
) -> Option<String> {
    vc.components
        .iter()
        .find(|c| component_renders_heading(&vc.name, c, mappings))
        .map(|c| c.name.clone())
}

/// Whether the view's page renders a view-level `<h1>` at all.
pub(crate) fn page_renders_heading(
    vc: &IfmlViewContainer,
    mappings: Option<&IfmlComponentMappings>,
) -> bool {
    page_heading_owner(vc, mappings).is_some()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use phero_ifml::{ChartKind, ChartSpec, ComponentSpec};

    use super::*;

    fn component(name: &str, component_type: &str, spec: Option<ComponentSpec>) -> IfmlComponent {
        IfmlComponent {
            name: name.to_string(),
            component_type: component_type.to_string(),
            mode: None,
            entity: Some("Customer".to_string()),
            fields: vec!["name".to_string()],
            fields_with_types: Vec::new(),
            filter: None,
            properties: HashMap::new(),
            events: Vec::new(),
            parts: Vec::new(),
            spec,
        }
    }

    fn details(name: &str) -> IfmlComponent {
        component(name, "details", None)
    }

    fn table(name: &str) -> IfmlComponent {
        component(
            name,
            "table",
            Some(ComponentSpec::Table(phero_ifml::TableSpec {
                columns: Vec::new(),
                pagination: false,
            })),
        )
    }

    fn list(name: &str) -> IfmlComponent {
        component(name, "list", None)
    }

    fn chart(name: &str) -> IfmlComponent {
        component(
            name,
            "chart",
            Some(ComponentSpec::Chart(ChartSpec {
                kind: ChartKind::Pie,
                label_field: None,
                value_fields: vec!["revenue".to_string()],
                hierarchy_field: None,
                label_fields: Vec::new(),
                detail_fields: Vec::new(),
                max_depth: None,
                aggregate: None,
            })),
        )
    }

    fn view(name: &str, label: Option<&str>, components: Vec<IfmlComponent>) -> IfmlViewContainer {
        IfmlViewContainer {
            name: name.to_string(),
            label: label.map(str::to_string),
            is_xor: false,
            is_default: false,
            is_landmark: false,
            is_modal: false,
            conditional_expression: None,
            conditional_expr_json: None,
            roles: Vec::new(),
            requires: Vec::new(),
            params: Vec::new(),
            components,
            events: Vec::new(),
            containers: Vec::new(),
        }
    }

    fn shadcn() -> IfmlComponentMappings {
        codegraph_config::built_in_pack("shadcn-svelte").expect("pack")
    }

    #[test]
    fn details_only_view_renders_no_heading() {
        let vc = view("FindingsView", None, vec![details("info")]);
        assert_eq!(page_heading_owner(&vc, None), None);
        assert!(!page_renders_heading(&vc, None));
    }

    #[test]
    fn form_and_chart_only_views_render_no_heading() {
        let form_view = view(
            "EditView",
            Some("Edit"),
            vec![component("editor", "form", None)],
        );
        let chart_view = view("ChartView", Some("Chart"), vec![chart("plot")]);
        for vc in [form_view, chart_view] {
            assert!(!page_renders_heading(&vc, None), "{vc:?}");
        }
    }

    #[test]
    fn details_then_table_hands_the_heading_to_the_table() {
        let vc = view(
            "FindingsView",
            None,
            vec![details("info"), table("findings")],
        );
        assert_eq!(page_heading_owner(&vc, None), Some("findings".to_string()));
        assert!(page_renders_heading(&vc, None));
    }

    #[test]
    fn two_tables_keep_the_first_component() {
        let vc = view(
            "Dashboard",
            Some("Dashboard"),
            vec![table("grid"), table("sidebar")],
        );
        assert_eq!(page_heading_owner(&vc, None), Some("grid".to_string()));
    }

    #[test]
    fn fallback_list_and_chart_mix_takes_the_list() {
        let vc = view(
            "Mixed",
            None,
            vec![chart("plot"), list("grid"), details("info")],
        );
        assert_eq!(page_heading_owner(&vc, None), Some("grid".to_string()));
    }

    #[test]
    fn mapped_details_component_renders_a_heading() {
        let vc = view("FindingsView", None, vec![details("info")]);
        assert!(!page_renders_heading(&vc, None));
        // The shadcn pack maps the `display` role onto its Card component —
        // the mapped branch renders the `<h1>`.
        assert_eq!(
            page_heading_owner(&vc, Some(&shadcn())),
            Some("info".to_string())
        );
    }

    #[test]
    fn specless_table_component_renders_nothing_so_cannot_own_the_heading() {
        // The template's table branch needs the typed payload; a bare
        // `type: table` renders no markup (and no `<h1>`).
        let vc = view("Bare", Some("Bare"), vec![component("grid", "table", None)]);
        assert!(!page_renders_heading(&vc, None));
    }

    #[test]
    fn container_nested_components_never_own_the_page_heading() {
        let mut vc = view("Wizard", Some("Wizard"), Vec::new());
        vc.containers = vec![view("Step", None, vec![table("grid")])];
        assert!(!page_renders_heading(&vc, None));
    }
}
