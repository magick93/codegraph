use std::path::Path;

use tera::Tera;

use crate::error::{Error, Result};

/// Initialize the Tera template engine from a template directory.
pub fn create_tera(template_dir: &Path) -> Result<Tera> {
    let glob = template_dir.join("**/*.tera").to_string_lossy().to_string();

    let mut tera = Tera::new(&glob).map_err(|e| Error::Template(e.to_string()))?;

    // Register custom filters
    tera.register_filter("snake_case", snake_case_filter);
    tera.register_filter("upper_camel", upper_camel_filter);
    tera.register_filter("pascal_case", pascal_case_filter);
    tera.register_filter("kebab_case", kebab_case_filter);
    tera.register_filter("pluralize", pluralize_filter);
    tera.register_filter("truncate_pg", truncate_pg_filter);
    tera.register_filter("dollar_quote", dollar_quote_filter);
    tera.register_filter("strip_pg_quotes", strip_pg_quotes_filter);
    tera.register_filter("quote_pg", quote_pg_filter);

    Ok(tera)
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
    let stripped = codegraph_naming::strip_type_suffix(s);
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
