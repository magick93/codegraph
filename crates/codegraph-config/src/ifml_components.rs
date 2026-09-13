use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Framework-agnostic semantic role of a UI slot. IFML describes what a slot
/// means (an action, a modal, a selection); design-system packs bind that
/// meaning to concrete widgets. Closed on purpose: unknown role strings in
/// TOML are a parse error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SemanticRole {
    ActionControl,
    NavigationControl,
    Field,
    SelectionField,
    Collection,
    ModalView,
    PresentationContainer,
    Display,
    Shell,
    Pagination,
}

/// Component mappings loaded from `ifml-components.toml`.
///
/// Each `[[component]]` entry maps IFML view components (matched by name,
/// type, kind, and/or view) to a handcrafted Svelte (or other framework)
/// component. Unmatched components fall back to the built-in templates.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IfmlComponentMappings {
    #[serde(default, rename = "component")]
    pub components: Vec<IfmlComponentMapping>,
}

/// One component mapping entry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IfmlComponentMapping {
    /// IFML view component name (e.g. `"grid"`). Highest priority selector.
    #[serde(default)]
    pub name: Option<String>,
    /// IFML component type (e.g. `"list"`, `"form"`, `"details"`, `"chart"`).
    #[serde(default, rename = "type")]
    pub component_type: Option<String>,
    /// Layout kind (e.g. `"table"`, `"form"`, `"details"`, `"chart"`, `"list"`).
    /// Matches the typed spec kind when present, else the component type.
    #[serde(default)]
    pub kind: Option<String>,
    /// Semantic slot role (e.g. `"selection-field"`, `"modal-view"`) matched
    /// against the computed role of the slot. Priority: below name/type,
    /// above kind.
    #[serde(default)]
    pub role: Option<SemanticRole>,
    /// Restricts the mapping to a single view container (e.g. `"CustomerList"`).
    #[serde(default)]
    pub view: Option<String>,
    /// Import path of the component (e.g. `"$lib/components/DataTable.svelte"`).
    pub path: String,
    /// Exported symbol name. Defaults to the last path segment minus extension.
    #[serde(default)]
    pub export: Option<String>,
    /// Test-id values rendered on the mapped component / fallback markup.
    #[serde(default)]
    pub testids: Option<HashMap<String, String>>,
}

impl IfmlComponentMappings {
    /// Merge project mappings with a built-in design-system pack.
    ///
    /// Project entries are kept ahead of the pack's so they shadow pack
    /// entries at the same resolution tier (lookup is first-match-wins);
    /// pack entries stay reachable for slots the project mappings do not
    /// cover.
    pub fn merge_with_pack(mut project: Self, pack: &Self) -> Self {
        project.components.extend(pack.components.iter().cloned());
        project
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        Self::parse_str(&raw).map_err(|e| format!("invalid mappings in {}: {e}", path.display()))
    }

    pub fn parse_str(raw: &str) -> Result<Self, String> {
        toml::from_str(raw).map_err(|e| e.to_string())
    }

    /// Resolve the mapping for a view component.
    ///
    /// Priority: component name → component type → kind. Entries carrying a
    /// `view` selector only match inside that view; within a tier the first
    /// matching entry wins.
    pub fn resolve(
        &self,
        view: &str,
        component: &str,
        component_type: &str,
        kind: &str,
    ) -> Option<&IfmlComponentMapping> {
        self.resolve_slot(view, component, component_type, kind, None)
    }

    /// Resolve the mapping for a UI slot.
    ///
    /// Priority: name → component type → semantic role → kind. `role` is the
    /// slot role computed by the generator (see `SemanticRole`); entries
    /// carrying a `view` selector only match inside that view; within a tier
    /// the first matching entry wins.
    pub fn resolve_slot(
        &self,
        view: &str,
        name: &str,
        component_type: &str,
        kind: &str,
        role: Option<SemanticRole>,
    ) -> Option<&IfmlComponentMapping> {
        let view_ok = |m: &IfmlComponentMapping| m.view.as_deref().is_none_or(|v| v == view);
        self.components
            .iter()
            .find(|m| m.name.as_deref() == Some(name) && view_ok(m))
            .or_else(|| {
                self.components
                    .iter()
                    .find(|m| m.component_type.as_deref() == Some(component_type) && view_ok(m))
            })
            .or_else(|| role.and_then(|r| self.resolve_by_role(view, r)))
            .or_else(|| {
                self.components
                    .iter()
                    .find(|m| kind_matches(m, component_type, kind) && view_ok(m))
            })
    }

    /// Resolve a mapping purely by semantic role (first match wins, `view`
    /// scoping applies).
    pub fn resolve_by_role(&self, view: &str, role: SemanticRole) -> Option<&IfmlComponentMapping> {
        self.components
            .iter()
            .find(|m| m.role == Some(role) && m.view.as_deref().is_none_or(|v| v == view))
    }
}

fn kind_matches(m: &IfmlComponentMapping, component_type: &str, kind: &str) -> bool {
    match m.kind.as_deref() {
        Some(k) => k == component_type || k == kind,
        None => false,
    }
}

/// Built-in design-system packs: (name, embedded TOML source).
pub const BUILT_IN_PACKS: &[(&str, &str)] =
    &[("shadcn-svelte", include_str!("packs/shadcn-svelte.toml"))];

/// Names of the built-in design-system packs, in registration order.
pub fn built_in_pack_names() -> impl Iterator<Item = &'static str> {
    BUILT_IN_PACKS.iter().map(|(name, _)| *name)
}

/// Parse the built-in design-system pack `name`.
///
/// Unknown names are an error naming the known packs — callers surface the
/// message rather than silently falling back to unmapped components.
pub fn built_in_pack(name: &str) -> Result<IfmlComponentMappings, String> {
    let source = BUILT_IN_PACKS
        .iter()
        .find(|(known, _)| *known == name)
        .map(|(_, source)| *source)
        .ok_or_else(|| {
            format!(
                "unknown IFML design system pack \"{name}\"; known packs: {}",
                built_in_pack_names().collect::<Vec<_>>().join(", ")
            )
        })?;
    IfmlComponentMappings::parse_str(source)
        .map_err(|e| format!("invalid built-in design system pack \"{name}\": {e}"))
}

impl IfmlComponentMapping {
    /// Exported symbol name: `export` when set, else the file stem of `path`.
    pub fn export_name(&self) -> &str {
        match self.export.as_deref() {
            Some(e) if !e.is_empty() => e,
            _ => Path::new(&self.path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&self.path),
        }
    }

    pub fn testid(&self, key: &str) -> Option<&str> {
        self.testids
            .as_ref()
            .and_then(|t| t.get(key))
            .map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mappings(raw: &str) -> IfmlComponentMappings {
        IfmlComponentMappings::parse_str(raw).unwrap()
    }

    #[test]
    fn parses_example_shape() {
        let m = mappings(
            r#"
[[component]]
kind = "table"
path = "$lib/components/DataTable.svelte"
export = "DataTable"
testids = { root = "data-table", row = "data-row" }

[[component]]
name = "grid"
view = "CustomerList"
path = "$lib/components/CustomerGrid.svelte"

[[component]]
type = "orgchart"
path = "$lib/components/OrgChart.svelte"
"#,
        );
        assert_eq!(m.components.len(), 3);
        assert_eq!(m.components[0].kind.as_deref(), Some("table"));
        assert_eq!(m.components[0].export_name(), "DataTable");
        assert_eq!(m.components[0].testid("root"), Some("data-table"));
        assert_eq!(m.components[0].testid("row"), Some("data-row"));
        assert_eq!(m.components[2].component_type.as_deref(), Some("orgchart"));
    }

    #[test]
    fn export_defaults_to_path_stem() {
        let m = mappings(
            r#"
[[component]]
kind = "table"
path = "$lib/components/DataTable.svelte"
"#,
        );
        assert_eq!(m.components[0].export_name(), "DataTable");
    }

    #[test]
    fn empty_file_yields_no_mappings() {
        let m = mappings("");
        assert!(m.components.is_empty());
        assert!(
            m.resolve("CustomerList", "grid", "list", "list").is_none(),
            "empty mappings resolve to nothing"
        );
    }

    #[test]
    fn name_beats_type_beats_kind() {
        let m = mappings(
            r#"
[[component]]
kind = "list"
path = "$lib/components/KindList.svelte"

[[component]]
type = "list"
path = "$lib/components/TypeList.svelte"

[[component]]
name = "grid"
path = "$lib/components/Grid.svelte"
"#,
        );
        let resolved = m.resolve("Any", "grid", "list", "list").unwrap();
        assert_eq!(resolved.path, "$lib/components/Grid.svelte");

        let resolved = m.resolve("Any", "other", "list", "list").unwrap();
        assert_eq!(resolved.path, "$lib/components/TypeList.svelte");

        let resolved = m.resolve("Any", "other", "tree", "list").unwrap();
        assert_eq!(resolved.path, "$lib/components/KindList.svelte");
    }

    #[test]
    fn kind_matches_spec_kind_and_component_type() {
        let m = mappings(
            r#"
[[component]]
kind = "table"
path = "$lib/components/DataTable.svelte"
"#,
        );
        assert!(m.resolve("V", "grid", "table", "table").is_some());
        assert!(m.resolve("V", "grid", "list", "table").is_some());
        assert!(m.resolve("V", "grid", "list", "list").is_none());
    }

    #[test]
    fn view_scoped_mapping_only_matches_that_view() {
        let m = mappings(
            r#"
[[component]]
name = "grid"
view = "CustomerList"
path = "$lib/components/CustomerGrid.svelte"
"#,
        );
        assert_eq!(
            m.resolve("CustomerList", "grid", "list", "list")
                .unwrap()
                .path,
            "$lib/components/CustomerGrid.svelte"
        );
        assert!(m.resolve("OtherView", "grid", "list", "list").is_none());
    }

    #[test]
    fn load_rejects_invalid_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ifml-components.toml");
        std::fs::write(&path, "[[component]]\npath = 123\n").unwrap();
        assert!(IfmlComponentMappings::load(&path).is_err());
    }

    #[test]
    fn role_parses_kebab_case() {
        let m = mappings(
            r#"
[[component]]
role = "selection-field"
path = "$lib/components/Select.svelte"

[[component]]
role = "modal-view"
path = "$lib/components/Dialog.svelte"
"#,
        );
        assert_eq!(m.components[0].role, Some(SemanticRole::SelectionField));
        assert_eq!(m.components[1].role, Some(SemanticRole::ModalView));
    }

    #[test]
    fn unknown_role_fails_parse() {
        let err = IfmlComponentMappings::parse_str(
            r#"
[[component]]
role = "button"
path = "$lib/components/Button.svelte"
"#,
        )
        .expect_err("unknown role must be a parse error");
        assert!(err.contains("unknown variant"), "{err}");
    }

    #[test]
    fn resolve_priority_name_type_role_kind() {
        let m = mappings(
            r#"
[[component]]
kind = "table"
path = "$lib/components/Kind.svelte"

[[component]]
role = "collection"
path = "$lib/components/Role.svelte"

[[component]]
type = "list"
path = "$lib/components/Type.svelte"

[[component]]
name = "grid"
path = "$lib/components/Name.svelte"
"#,
        );
        let slot = |name: &str, component_type: &str, kind: &str, role: Option<SemanticRole>| {
            m.resolve_slot("V", name, component_type, kind, role)
                .unwrap()
                .path
                .to_string()
        };
        assert_eq!(
            slot("grid", "list", "table", Some(SemanticRole::Collection)),
            "$lib/components/Name.svelte"
        );
        assert_eq!(
            slot("other", "list", "table", Some(SemanticRole::Collection)),
            "$lib/components/Type.svelte"
        );
        assert_eq!(
            slot("other", "tree", "table", Some(SemanticRole::Collection)),
            "$lib/components/Role.svelte"
        );
        assert_eq!(
            slot("other", "tree", "table", None),
            "$lib/components/Kind.svelte"
        );
    }

    #[test]
    fn view_scoped_role_entry_only_matches_that_view() {
        let m = mappings(
            r#"
[[component]]
role = "collection"
view = "CustomerList"
path = "$lib/components/CustomerList.svelte"
"#,
        );
        let resolved = m.resolve_by_role("CustomerList", SemanticRole::Collection);
        assert_eq!(
            resolved.unwrap().path,
            "$lib/components/CustomerList.svelte"
        );
        assert!(m
            .resolve_by_role("OtherView", SemanticRole::Collection)
            .is_none());
        assert!(m
            .resolve_slot(
                "OtherView",
                "grid",
                "tree",
                "tree",
                Some(SemanticRole::Collection)
            )
            .is_none());
    }

    #[test]
    fn resolve_by_role_picks_first_match() {
        let m = mappings(
            r#"
[[component]]
role = "action-control"
path = "$lib/components/First.svelte"

[[component]]
role = "action-control"
path = "$lib/components/Second.svelte"
"#,
        );
        assert_eq!(
            m.resolve_by_role("Any", SemanticRole::ActionControl)
                .unwrap()
                .path,
            "$lib/components/First.svelte"
        );
        assert!(m
            .resolve_by_role("Any", SemanticRole::NavigationControl)
            .is_none());
    }

    #[test]
    fn resolve_delegates_to_slot_without_role() {
        let m = mappings(
            r#"
[[component]]
name = "grid"
path = "$lib/components/Grid.svelte"
"#,
        );
        assert_eq!(
            m.resolve("V", "grid", "list", "list").unwrap().path,
            m.resolve_slot("V", "grid", "list", "list", None)
                .unwrap()
                .path
        );
    }

    #[test]
    fn built_in_pack_names_include_shadcn_svelte() {
        assert!(built_in_pack_names().any(|name| name == "shadcn-svelte"));
    }

    #[test]
    fn shadcn_svelte_pack_parses_and_covers_roles() {
        let pack = built_in_pack("shadcn-svelte").unwrap();
        assert!(!pack.components.is_empty());
        for role in [
            SemanticRole::ActionControl,
            SemanticRole::NavigationControl,
            SemanticRole::Field,
            SemanticRole::SelectionField,
            SemanticRole::Collection,
            SemanticRole::ModalView,
            SemanticRole::PresentationContainer,
            SemanticRole::Display,
            SemanticRole::Shell,
            SemanticRole::Pagination,
        ] {
            assert!(
                pack.resolve_by_role("AnyView", role).is_some(),
                "pack misses role {role:?}"
            );
        }
        let button = pack.resolve_by_role("AnyView", SemanticRole::ActionControl);
        assert_eq!(
            button.unwrap().path,
            "$lib/components/ui/button/button.svelte"
        );
    }

    #[test]
    fn unknown_pack_name_is_an_error() {
        let err = built_in_pack("material").expect_err("unknown pack must error");
        assert!(
            err.contains("unknown IFML design system pack \"material\""),
            "{err}"
        );
        assert!(err.contains("shadcn-svelte"), "{err}");
    }

    #[test]
    fn merge_keeps_project_entries_ahead_of_pack() {
        let project = mappings(
            r#"
[[component]]
role = "collection"
path = "$lib/components/MyTable.svelte"
"#,
        );
        let pack = built_in_pack("shadcn-svelte").unwrap();
        let merged = IfmlComponentMappings::merge_with_pack(project, &pack);
        let resolved = merged
            .resolve_slot("V", "grid", "list", "table", Some(SemanticRole::Collection))
            .unwrap();
        assert_eq!(resolved.path, "$lib/components/MyTable.svelte");
    }

    #[test]
    fn merge_falls_through_to_pack_when_project_silent() {
        let project = mappings(
            r#"
[[component]]
name = "grid"
path = "$lib/components/Grid.svelte"
"#,
        );
        let pack = built_in_pack("shadcn-svelte").unwrap();
        let merged = IfmlComponentMappings::merge_with_pack(project, &pack);
        assert_eq!(
            merged
                .resolve_slot("V", "grid", "list", "table", Some(SemanticRole::Collection))
                .unwrap()
                .path,
            "$lib/components/Grid.svelte"
        );
        assert_eq!(
            merged
                .resolve_slot(
                    "V",
                    "save",
                    "button",
                    "button",
                    Some(SemanticRole::ActionControl)
                )
                .unwrap()
                .path,
            "$lib/components/ui/button/button.svelte"
        );
        assert_eq!(
            merged
                .resolve("V", "other", "table", "table")
                .unwrap()
                .export_name(),
            "Table"
        );
    }

    #[test]
    fn merge_with_empty_project_uses_pack() {
        let project = mappings("");
        let pack = built_in_pack("shadcn-svelte").unwrap();
        let merged = IfmlComponentMappings::merge_with_pack(project, &pack);
        assert_eq!(
            merged
                .resolve_by_role("V", SemanticRole::ModalView)
                .unwrap()
                .export_name(),
            "Dialog"
        );
    }
}
