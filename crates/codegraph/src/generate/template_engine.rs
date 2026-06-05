use std::path::Path;

use tera::Tera;

use crate::error::{Error, Result};

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

/// Initialize the Tera template engine from a template directory.
pub fn create_tera(template_dir: &Path) -> Result<Tera> {
    let glob = template_dir.join("**/*.tera").to_string_lossy().to_string();
    let mut tera = Tera::new(&glob).map_err(|e| Error::Template(e.to_string()))?;
    register_filters(&mut tera);
    Ok(tera)
}

/// Create a Tera instance with codegraph's built-in templates as defaults,
/// plus zero or more override directories that shadow templates by name.
/// Later directories take precedence over earlier ones.
pub fn create_tera_with_overrides(override_dirs: &[&Path]) -> Result<Tera> {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let glob = base.join("**/*.tera").to_string_lossy().to_string();
    let mut tera = Tera::new(&glob).map_err(|e| Error::Template(e.to_string()))?;
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
    for entry in walkdir::WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
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
    Ok(tera::Value::String(codegraph_naming::to_pascal_case(&stripped)))
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
    Ok(tera::Value::String(codegraph_naming::truncate_pg_identifier(s)))
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
