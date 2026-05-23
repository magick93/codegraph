/// Strip a configurable suffix from schema titles for cleaner generated names.
///
/// HR Open Standards schema titles universally end in "Type" (e.g. "PersonType",
/// "WorkerType"). This produces noisy generated code (struct PersonType, table
/// person_type, etc.). This function strips the given suffix so generated artifacts
/// use clean names: Person, Worker, person, worker.
///
/// # Examples
/// ```
/// use codegraph_naming::strip_suffix;
/// assert_eq!(strip_suffix("PersonType", "Type"), "Person");
/// assert_eq!(strip_suffix("WorkerType", "Type"), "Worker");
/// assert_eq!(strip_suffix("CountryCodeList", "Type"), "CountryCodeList");
/// assert_eq!(strip_suffix("Person", "Type"), "Person");
/// ```
pub fn strip_suffix(title: &str, suffix: &str) -> String {
    let stripped = title.strip_suffix(suffix).unwrap_or(title);
    // Remove characters that are invalid in Rust identifiers (e.g. hyphens in "LER-RS", @ in "@context")
    stripped.replace(['-', '@'], "").to_string()
}

/// Strip the "Type" suffix from HR Open schema titles for cleaner generated names.
///
/// Convenience wrapper around [`strip_suffix`] that strips the default "Type" suffix.
/// Prefer `strip_suffix(title, &config.defaults.type_suffix)` in new code.
#[deprecated(since = "0.2.0", note = "use strip_suffix(title, &config.defaults.type_suffix) instead")]
pub fn strip_type_suffix(title: &str) -> String {
    strip_suffix(title, "Type")
}

/// Rust reserved keywords that need `r#` escaping when used as identifiers.
const RUST_KEYWORDS: &[&str] = &[
    "abstract", "as", "async", "await", "become", "box", "break", "const", "continue", "crate",
    "do", "dyn", "else", "enum", "extern", "false", "final", "fn", "for", "if", "impl", "in",
    "let", "loop", "macro", "match", "mod", "move", "mut", "override", "priv", "pub", "ref",
    "return", "self", "static", "struct", "super", "trait", "true", "try", "type", "typeof",
    "unsafe", "unsized", "use", "virtual", "where", "while", "yield",
];

/// Escape a Rust reserved keyword by prefixing with `r#`.
pub fn escape_rust_keyword(name: &str) -> String {
    if RUST_KEYWORDS.contains(&name) {
        format!("r#{}", name)
    } else {
        name.to_string()
    }
}

/// Escape a Rust reserved keyword for use as a module name by appending `_mod`.
///
/// Unlike `escape_rust_keyword` which uses the `r#` prefix, this produces a plain
/// identifier that is safe in all contexts — including proc-macro path strings
/// (e.g., utoipa's `#[openapi(paths(...))]`) where `r#` is not supported.
pub fn escape_module_keyword(name: &str) -> String {
    if RUST_KEYWORDS.contains(&name) {
        format!("{}_mod", name)
    } else if name.starts_with(|c: char| c.is_ascii_digit()) {
        format!("n_{}", name)
    } else {
        name.to_string()
    }
}

/// Convert UpperCamelCase to snake_case.
pub fn to_snake_case(name: &str) -> String {
    heck::AsSnakeCase(name).to_string()
}

/// Convert to PascalCase (UpperCamelCase).
///
/// Handles snake_case, kebab-case, and single-word inputs:
/// - `recruiting` → `Recruiting`
/// - `time_card` → `TimeCard`
/// - `common` → `Common`
pub fn to_pascal_case(name: &str) -> String {
    heck::AsPascalCase(name).to_string()
}

/// Convert to kebab-case (for URL path segments).
pub fn to_kebab_case(name: &str) -> String {
    heck::AsKebabCase(name).to_string()
}

/// PostgreSQL reserved words that cause syntax errors when used as unquoted identifiers.
///
/// Source: PostgreSQL 16 documentation, Appendix C — SQL Key Words (reserved).
const PG_RESERVED: &[&str] = &[
    "all",
    "analyse",
    "analyze",
    "and",
    "any",
    "array",
    "as",
    "asc",
    "asymmetric",
    "authorization",
    "between",
    "binary",
    "both",
    "case",
    "cast",
    "check",
    "collate",
    "collation",
    "column",
    "concurrently",
    "constraint",
    "create",
    "cross",
    "current_catalog",
    "current_date",
    "current_role",
    "current_schema",
    "current_time",
    "current_timestamp",
    "current_user",
    "default",
    "deferrable",
    "desc",
    "distinct",
    "do",
    "else",
    "end",
    "except",
    "false",
    "fetch",
    "for",
    "foreign",
    "freeze",
    "from",
    "full",
    "grant",
    "group",
    "having",
    "ilike",
    "in",
    "initially",
    "inner",
    "intersect",
    "into",
    "is",
    "isnull",
    "join",
    "lateral",
    "leading",
    "left",
    "like",
    "limit",
    "localtime",
    "localtimestamp",
    "natural",
    "not",
    "notnull",
    "null",
    "offset",
    "on",
    "only",
    "or",
    "order",
    "outer",
    "overlaps",
    "placing",
    "primary",
    "references",
    "returning",
    "right",
    "select",
    "session_user",
    "similar",
    "some",
    "start",
    "symmetric",
    "table",
    "tablesample",
    "then",
    "to",
    "trailing",
    "true",
    "union",
    "unique",
    "user",
    "using",
    "variadic",
    "verbose",
    "when",
    "where",
    "window",
    "with",
];

/// Wrap a PostgreSQL column name in double quotes if it clashes with a reserved word.
///
/// # Examples
/// ```
/// use codegraph_naming::quote_pg_column;
/// assert_eq!(quote_pg_column("order"), "\"order\"");
/// assert_eq!(quote_pg_column("start"), "\"start\"");
/// assert_eq!(quote_pg_column("name"), "name");
/// ```
pub fn quote_pg_column(name: &str) -> String {
    if PG_RESERVED.contains(&name.to_ascii_lowercase().as_str()) {
        format!("\"{}\"", name)
    } else {
        name.to_string()
    }
}

/// PostgreSQL maximum identifier length (NAMEDATALEN - 1).
const PG_MAX_IDENT: usize = 63;

/// Truncate a PostgreSQL identifier to fit within the 63-character limit.
///
/// If the name is already ≤63 chars, it is returned as-is. Otherwise, the name
/// is truncated and a 7-character hex hash suffix is appended to ensure uniqueness.
/// The hash is deterministic (FNV-1a) so the same input always produces the same output.
///
/// # Examples
/// ```
/// use codegraph_naming::truncate_pg_identifier;
/// let short = "candidate_profiles";
/// assert_eq!(truncate_pg_identifier(short), short);
///
/// let long = "candidate_distribution_guidelines_distribute_to_communication_address_country_sub_divisions";
/// let result = truncate_pg_identifier(long);
/// assert!(result.len() <= 63);
/// assert!(result.starts_with("candidate_distribution_guidelines_distribute_to_communi"));
/// ```
pub fn truncate_pg_identifier(name: &str) -> String {
    if name.len() <= PG_MAX_IDENT {
        return name.to_string();
    }
    // FNV-1a 64-bit hash of the full name for deterministic uniqueness
    let hash = fnv1a_64(name.as_bytes());
    let suffix = format!("_{:07x}", hash & 0x0FFF_FFFF); // 7 hex chars + underscore = 8
    let prefix_len = PG_MAX_IDENT - suffix.len();
    // Trim trailing underscores from the prefix to keep names clean
    let prefix = name[..prefix_len].trim_end_matches('_');
    format!("{}{}", prefix, suffix)
}

/// FNV-1a 64-bit hash (no external dependencies).
fn fnv1a_64(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Derive a display name from a CamelCase type name.
/// "PersonBaseType" → "Person Base", "GivenName" → "Given Name"
pub fn to_display_name(name: &str) -> String {
    let stripped = strip_suffix(name, "Type");
    let mut result = String::new();
    for (i, ch) in stripped.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            result.push(' ');
        }
        result.push(ch);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_suffix() {
        assert_eq!(strip_suffix("PersonType", "Type"), "Person");
        assert_eq!(strip_suffix("WorkerType", "Type"), "Worker");
        assert_eq!(strip_suffix("CountryCodeList", "Type"), "CountryCodeList");
        assert_eq!(strip_suffix("Person", "Type"), "Person");
        assert_eq!(strip_suffix("PersonView", "View"), "Person");
        assert_eq!(strip_suffix("CustomEntitySuf", "Suf"), "CustomEntity");
    }

    #[allow(deprecated)]
    #[test]
    fn test_strip_type_suffix_backward_compat() {
        assert_eq!(strip_type_suffix("PersonType"), "Person");
        assert_eq!(strip_type_suffix("WorkerType"), "Worker");
        assert_eq!(strip_type_suffix("CountryCodeList"), "CountryCodeList");
        assert_eq!(strip_type_suffix("Person"), "Person");
    }

    #[test]
    fn test_escape_rust_keyword() {
        assert_eq!(escape_rust_keyword("type"), "r#type");
        assert_eq!(escape_rust_keyword("name"), "name");
    }

    #[test]
    fn test_escape_module_keyword() {
        assert_eq!(escape_module_keyword("type"), "type_mod");
        assert_eq!(escape_module_keyword("3d"), "n_3d");
        assert_eq!(escape_module_keyword("name"), "name");
    }

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("PersonBase"), "person_base");
        assert_eq!(to_snake_case("GivenName"), "given_name");
    }

    #[test]
    fn test_to_kebab_case() {
        assert_eq!(to_kebab_case("PersonBase"), "person-base");
        assert_eq!(to_kebab_case("MilitaryService"), "military-service");
    }

    #[test]
    fn test_to_display_name() {
        assert_eq!(to_display_name("PersonBaseType"), "Person Base");
        assert_eq!(to_display_name("GivenName"), "Given Name");
    }

    #[test]
    fn test_quote_pg_column_reserved() {
        assert_eq!(quote_pg_column("order"), "\"order\"");
        assert_eq!(quote_pg_column("start"), "\"start\"");
        assert_eq!(quote_pg_column("end"), "\"end\"");
        assert_eq!(quote_pg_column("user"), "\"user\"");
        assert_eq!(quote_pg_column("select"), "\"select\"");
    }

    #[test]
    fn test_quote_pg_column_safe() {
        assert_eq!(quote_pg_column("name"), "name");
        assert_eq!(quote_pg_column("given_name"), "given_name");
        assert_eq!(quote_pg_column("id"), "id");
        assert_eq!(quote_pg_column("created_at"), "created_at");
    }

    #[test]
    fn test_truncate_pg_identifier_short() {
        let name = "candidate_profiles";
        assert_eq!(truncate_pg_identifier(name), name);
    }

    #[test]
    fn test_truncate_pg_identifier_exactly_63() {
        let name = "a".repeat(63);
        assert_eq!(truncate_pg_identifier(&name), name);
    }

    #[test]
    fn test_truncate_pg_identifier_long() {
        let name = "candidate_distribution_guidelines_distribute_to_communication_address_country_sub_divisions";
        let result = truncate_pg_identifier(name);
        assert!(result.len() <= 63, "result length {} > 63", result.len());
        // Must be deterministic
        assert_eq!(result, truncate_pg_identifier(name));
    }

    #[test]
    fn test_truncate_pg_identifier_uniqueness() {
        // Two different long names should produce different truncated results
        let a = "candidate_profiles_application_process_history_associated_parties_organization_communication";
        let b = "candidate_profiles_application_process_history_associated_parties_organization_communication_address";
        let ra = truncate_pg_identifier(a);
        let rb = truncate_pg_identifier(b);
        assert_ne!(ra, rb, "different inputs must produce different outputs");
        assert!(ra.len() <= 63);
        assert!(rb.len() <= 63);
    }
}
