use std::path::{Path, PathBuf};

use codegraph_config::built_in_pack_names;
use tera::Tera;

use crate::error::{Error, Result};

include!(concat!(env!("OUT_DIR"), "/embedded_templates.rs"));

fn register_filters(tera: &mut Tera) {
    tera.register_filter("snake_case", snake_case_filter);
    tera.register_filter("upper_camel", upper_camel_filter);
    tera.register_filter("pascal_case", pascal_case_filter);
    tera.register_filter("kebab_case", kebab_case_filter);
    tera.register_filter("pluralize", pluralize_filter);
    tera.register_filter("truncate_pg", truncate_pg_filter);
    tera.register_filter("dollar_quote", dollar_quote_filter);
    tera.register_filter("strip_pg_quotes", strip_pg_quotes_filter);
    tera.register_filter("quote_pg", quote_pg_filter);
}

/// Initialize the Tera template engine with embedded templates.
pub fn create_tera(_template_dir: &Path) -> Result<Tera> {
    let mut tera = Tera::default();
    add_embedded_templates(&mut tera).map_err(|e| Error::Template(e.to_string()))?;
    register_filters(&mut tera);
    Ok(tera)
}

/// Create a Tera instance with codegraph's built-in embedded templates,
/// plus zero or more override directories that shadow templates by name.
/// Later directories take precedence over earlier ones.
pub fn create_tera_with_overrides(override_dirs: &[&Path]) -> Result<Tera> {
    let mut tera = Tera::default();
    add_embedded_templates(&mut tera).map_err(|e| Error::Template(e.to_string()))?;
    for dir in override_dirs {
        if dir.exists() {
            merge_tera_dir(&mut tera, dir)?;
        }
    }
    register_filters(&mut tera);
    Ok(tera)
}

/// Walk a directory and add every `.tera` file to the Tera engine,
/// shadowing any existing template with the same name.
fn merge_tera_dir(tera: &mut Tera, dir: &Path) -> Result<()> {
    for entry in walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("tera") {
            continue;
        }
        let relative = path
            .strip_prefix(dir)
            .expect("walkdir path must be under base dir");
        let name = relative.to_string_lossy().replace('\\', "/");
        let content = std::fs::read_to_string(path)
            .map_err(|e| Error::Template(format!("read override {name}: {e}")))?;
        tera.add_raw_template(&name, &content)
            .map_err(|e| Error::Template(format!("add override {name}: {e}")))?;
    }
    Ok(())
}

/// Root of the per-pack template override subtrees
/// (`templates/ifml/packs/<pack-name>/...`). Names inside a pack mirror the
/// built-in template names (e.g. `ifml/svelte/layout.tera`).
fn pack_template_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("templates")
        .join("ifml")
        .join("packs")
}

/// Template override directory shipped for built-in design-system pack
/// `name`, if the pack ships any.
///
/// `Ok(None)` means the pack is known but only carries component mappings;
/// `Err` rejects unknown pack names so callers surface typos instead of
/// silently running unpackaged.
pub fn built_in_pack_template_dir(name: &str) -> Result<Option<PathBuf>> {
    if !built_in_pack_names().any(|known| known == name) {
        return Err(Error::Config(format!(
            "unknown IFML design system pack \"{name}\"; known packs: {}",
            built_in_pack_names().collect::<Vec<_>>().join(", ")
        )));
    }
    let dir = pack_template_root().join(name);
    Ok(dir.is_dir().then_some(dir))
}

/// Create the Tera engine for a generation run: embedded built-ins first,
/// then the selected design-system pack's template overrides, then the
/// project's template override dirs. Later layers shadow earlier ones, so
/// precedence is project template-dir > pack templates > built-ins.
pub fn create_tera_for_run(design_system: Option<&str>, override_dirs: &[&Path]) -> Result<Tera> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(name) = design_system.filter(|name| !name.is_empty()) {
        if let Some(dir) = built_in_pack_template_dir(name)? {
            dirs.push(dir);
        }
    }
    dirs.extend(override_dirs.iter().map(|p| p.to_path_buf()));
    let refs: Vec<&Path> = dirs.iter().map(|p| p.as_path()).collect();
    create_tera_with_overrides(&refs)
}

fn snake_case_filter(
    value: &tera::Value,
    _args: &std::collections::HashMap<String, tera::Value>,
) -> tera::Result<tera::Value> {
    let s = value
        .as_str()
        .ok_or_else(|| tera::Error::msg("snake_case filter expects a string"))?;
    Ok(tera::Value::String(codegraph_naming::to_snake_case(s)))
}

fn upper_camel_filter(
    value: &tera::Value,
    _args: &std::collections::HashMap<String, tera::Value>,
) -> tera::Result<tera::Value> {
    let s = value
        .as_str()
        .ok_or_else(|| tera::Error::msg("upper_camel filter expects a string"))?;
    let stripped = codegraph_naming::strip_suffix(s, "Type");
    Ok(tera::Value::String(codegraph_naming::to_pascal_case(
        &stripped,
    )))
}

fn pascal_case_filter(
    value: &tera::Value,
    _args: &std::collections::HashMap<String, tera::Value>,
) -> tera::Result<tera::Value> {
    let s = value
        .as_str()
        .ok_or_else(|| tera::Error::msg("pascal_case filter expects a string"))?;
    Ok(tera::Value::String(codegraph_naming::to_pascal_case(s)))
}

fn kebab_case_filter(
    value: &tera::Value,
    _args: &std::collections::HashMap<String, tera::Value>,
) -> tera::Result<tera::Value> {
    let s = value
        .as_str()
        .ok_or_else(|| tera::Error::msg("kebab_case filter expects a string"))?;
    Ok(tera::Value::String(codegraph_naming::to_kebab_case(s)))
}

fn truncate_pg_filter(
    value: &tera::Value,
    _args: &std::collections::HashMap<String, tera::Value>,
) -> tera::Result<tera::Value> {
    let s = value
        .as_str()
        .ok_or_else(|| tera::Error::msg("truncate_pg filter expects a string"))?;
    Ok(tera::Value::String(
        codegraph_naming::truncate_pg_identifier(s),
    ))
}

fn pluralize_filter(
    value: &tera::Value,
    _args: &std::collections::HashMap<String, tera::Value>,
) -> tera::Result<tera::Value> {
    let s = value
        .as_str()
        .ok_or_else(|| tera::Error::msg("pluralize filter expects a string"))?;
    // Simple pluralization: add 's' unless already ends with 's'
    let plural = if s.ends_with('s') {
        format!("{}es", s)
    } else if s.ends_with('y') && !s.ends_with("ey") && !s.ends_with("ay") && !s.ends_with("oy") {
        format!("{}ies", &s[..s.len() - 1])
    } else {
        format!("{}s", s)
    };
    Ok(tera::Value::String(plural))
}

/// Strip surrounding double-quotes from a PostgreSQL identifier.
/// Used in constraint names where quoted column names (e.g. `"language"`)
/// must appear without quotes (e.g. `fk_table_language` not `fk_table_"language"`).
fn strip_pg_quotes_filter(
    value: &tera::Value,
    _args: &std::collections::HashMap<String, tera::Value>,
) -> tera::Result<tera::Value> {
    let s = value
        .as_str()
        .ok_or_else(|| tera::Error::msg("strip_pg_quotes filter expects a string"))?;
    let stripped = s.replace('"', "");
    Ok(tera::Value::String(stripped))
}

/// Double-quote a PostgreSQL identifier if it is a reserved word.
/// E.g. `order` → `"order"`, `candidate` → `candidate`.
fn quote_pg_filter(
    value: &tera::Value,
    _args: &std::collections::HashMap<String, tera::Value>,
) -> tera::Result<tera::Value> {
    let s = value
        .as_str()
        .ok_or_else(|| tera::Error::msg("quote_pg filter expects a string"))?;
    Ok(tera::Value::String(codegraph_naming::quote_pg_column(s)))
}

fn dollar_quote_filter(
    value: &tera::Value,
    _args: &std::collections::HashMap<String, tera::Value>,
) -> tera::Result<tera::Value> {
    let s = value
        .as_str()
        .ok_or_else(|| tera::Error::msg("dollar_quote filter expects a string"))?;
    // Use single-quote escaping instead of dollar-quoting.
    // Supabase CLI's migration runner mishandles $$$$ (empty dollar-quoted strings),
    // causing INSERT statements in codelist migrations to silently fail.
    let escaped = s.replace('\'', "''");
    Ok(tera::Value::String(format!("'{}'", escaped)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Render the IFML layout template with a minimal shell context — the
    /// smallest template whose pack override proves the merge precedence.
    fn render_layout(tera: &Tera) -> String {
        let ctx = tera::Context::from_serialize(serde_json::json!({
            "shell": {
                "import": { "export_name": "Nav", "import_path": "$lib/Nav.svelte" },
                "testid": null,
                "items": [{ "label": "Home", "href_attr": "href={/}" }]
            }
        }))
        .expect("test shell context serializes");
        tera.render("ifml/svelte/layout.tera", &ctx)
            .expect("layout template renders")
    }

    #[test]
    fn built_in_pack_template_dir_resolves_shipped_shadcn_svelte_dir() {
        let dir = built_in_pack_template_dir("shadcn-svelte")
            .expect("known pack resolves")
            .expect("shadcn-svelte ships template overrides");
        assert!(dir.is_dir(), "template dir must exist on disk: {dir:?}");
        assert!(dir.join("ifml/svelte/layout.tera").is_file());
    }

    #[test]
    fn built_in_pack_template_dir_unknown_pack_is_an_error() {
        let err = built_in_pack_template_dir("material").expect_err("unknown pack must error");
        assert!(
            err.to_string()
                .contains("unknown IFML design system pack \"material\""),
            "{err}"
        );
    }

    #[test]
    fn pack_templates_apply_without_project_overrides() {
        let tera = create_tera_for_run(Some("shadcn-svelte"), &[]).unwrap();
        assert!(
            render_layout(&tera).contains("data-slot=\"navigation-menu\""),
            "pack override must shadow the built-in layout"
        );
    }

    #[test]
    fn no_pack_selection_renders_builtin_layout() {
        let tera = create_tera_for_run(None, &[]).unwrap();
        assert!(
            !render_layout(&tera).contains("data-slot"),
            "built-in layout must stay pack-free"
        );
    }

    #[test]
    fn pack_beats_builtin_and_project_dir_beats_pack() {
        let project = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(project.path().join("ifml/svelte")).unwrap();
        std::fs::write(
            project.path().join("ifml/svelte/layout.tera"),
            "PROJECT {{ shell.import.export_name }}",
        )
        .unwrap();

        let tera = create_tera_for_run(Some("shadcn-svelte"), &[project.path()]).unwrap();
        let out = render_layout(&tera);
        assert!(out.contains("PROJECT"), "project override must win: {out}");
        assert!(
            !out.contains("data-slot"),
            "pack template must not leak past the project override: {out}"
        );
    }

    #[test]
    fn unknown_design_system_is_an_error_in_create_tera_for_run() {
        assert!(create_tera_for_run(Some("material"), &[]).is_err());
    }

    #[test]
    fn empty_design_system_string_means_no_pack() {
        let tera = create_tera_for_run(Some(""), &[]).unwrap();
        assert!(!render_layout(&tera).contains("data-slot"));
    }
}
