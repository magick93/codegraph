use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

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
        let view_ok = |m: &IfmlComponentMapping| m.view.as_deref().is_none_or(|v| v == view);
        self.components
            .iter()
            .find(|m| m.name.as_deref() == Some(component) && view_ok(m))
            .or_else(|| {
                self.components
                    .iter()
                    .find(|m| m.component_type.as_deref() == Some(component_type) && view_ok(m))
            })
            .or_else(|| {
                self.components
                    .iter()
                    .find(|m| kind_matches(m, component_type, kind) && view_ok(m))
            })
    }
}

fn kind_matches(m: &IfmlComponentMapping, component_type: &str, kind: &str) -> bool {
    match m.kind.as_deref() {
        Some(k) => k == component_type || k == kind,
        None => false,
    }
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
}
