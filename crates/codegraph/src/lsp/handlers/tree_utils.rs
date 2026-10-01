use std::collections::HashMap;
use std::sync::LazyLock;

use auto_lsp::lsp_types::*;
use auto_lsp::tree_sitter;
use auto_lsp::tree_sitter::{Query, QueryCursor, StreamingIterator};

use crate::lsp::state::{GrafeoState, GRAFE};
pub(super) fn with_grafe<F, R>(f: F) -> R
where
    F: FnOnce(Option<&GrafeoState>) -> R,
{
    let guard = GRAFE
        .get()
        .map(|l| l.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
    f(guard.as_ref().and_then(|g| g.as_ref()))
}

pub(super) static IFML_LANG: LazyLock<tree_sitter::Language> =
    LazyLock::new(tree_sitter_ifml::language);

pub(super) static VIEW_DECL_QUERY: LazyLock<Query> = LazyLock::new(|| {
    Query::new(&IFML_LANG, r"(view_declaration (string) @view-name)")
        .expect("Failed to create view declaration query")
});

static DATA_REF_QUERY: LazyLock<Query> = LazyLock::new(|| {
    Query::new(
        &IFML_LANG,
        r"(property_assignment key: (identifier) @key value: (value_expression (expression (identifier) @val)))",
    )
    .expect("Failed to create data ref query")
});

static NAVIGATE_BINDING_QUERY: LazyLock<Query> = LazyLock::new(|| {
    Query::new(
        &IFML_LANG,
        r"(navigate_action (string) @target (parameter_binding (binding_pair key: (identifier) @binding.key)))",
    )
    .expect("Failed to create navigate binding query")
});

static MODULE_DECL_QUERY: LazyLock<Query> = LazyLock::new(|| {
    Query::new(&IFML_LANG, r"(module_declaration (string) @module-name)")
        .expect("Failed to create module declaration query")
});

pub(super) fn extract_view_names(source: &[u8], root: &tree_sitter::Node) -> Vec<String> {
    let mut names = Vec::new();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&VIEW_DECL_QUERY, *root, source);

    while let Some(m) = matches.next() {
        for capture in m.captures {
            if let Ok(name) = capture.node.utf8_text(source) {
                names.push(name.trim_matches('"').to_string());
            }
        }
    }

    names
}

pub(super) fn extract_views_with_params(
    source: &[u8],
    root: &tree_sitter::Node,
) -> HashMap<String, Vec<String>> {
    let mut views: HashMap<String, Vec<String>> = HashMap::new();
    let mut cursor = root.walk();

    if !cursor.goto_first_child() {
        return views;
    }

    loop {
        let node = cursor.node();
        if node.kind() == "view_declaration" {
            let mut view_name = String::new();
            let mut params: Vec<String> = Vec::new();

            let mut vc = node.walk();
            if vc.goto_first_child() {
                loop {
                    let child = vc.node();
                    match child.kind() {
                        "string" if view_name.is_empty() => {
                            if let Ok(name) = child.utf8_text(source) {
                                view_name = name.trim_matches('"').to_string();
                            }
                        }
                        "view_body" => {
                            let mut bc = child.walk();
                            if bc.goto_first_child() {
                                loop {
                                    let body_child = bc.node();
                                    if body_child.kind() == "params_block" {
                                        let mut pc = body_child.walk();
                                        if pc.goto_first_child() {
                                            loop {
                                                let pb_child = pc.node();
                                                if pb_child.kind() == "parameter_block" {
                                                    let mut pbc = pb_child.walk();
                                                    if pbc.goto_first_child() {
                                                        loop {
                                                            let decl = pbc.node();
                                                            if decl.kind() == "parameter_decl" {
                                                                let mut dc = decl.walk();
                                                                if dc.goto_first_child() {
                                                                    loop {
                                                                        let param_child = dc.node();
                                                                        if param_child.kind()
                                                                            == "identifier"
                                                                        {
                                                                            if let Ok(name) =
                                                                                param_child
                                                                                    .utf8_text(
                                                                                        source,
                                                                                    )
                                                                            {
                                                                                params.push(
                                                                                    name.to_string(
                                                                                    ),
                                                                                );
                                                                            }
                                                                            break;
                                                                        }
                                                                        if !dc.goto_next_sibling() {
                                                                            break;
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                            if !pbc.goto_next_sibling() {
                                                                break;
                                                            }
                                                        }
                                                    }
                                                }
                                                if !pc.goto_next_sibling() {
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                    if !bc.goto_next_sibling() {
                                        break;
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                    if !vc.goto_next_sibling() {
                        break;
                    }
                }
            }

            if !view_name.is_empty() {
                views.insert(view_name, params);
            }
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }

    views
}

pub(super) fn extract_navigate_bindings(
    source: &[u8],
    root: &tree_sitter::Node,
) -> Vec<(String, Vec<String>)> {
    let mut results = Vec::new();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&NAVIGATE_BINDING_QUERY, *root, source);

    while let Some(m) = matches.next() {
        let mut target = String::new();
        let mut keys = Vec::new();

        for capture in m.captures {
            let name = NAVIGATE_BINDING_QUERY.capture_names()[capture.index as usize];
            match name {
                "target" => {
                    target = capture
                        .node
                        .utf8_text(source)
                        .ok()
                        .map(|s: &str| s.trim_matches('"').to_string())
                        .unwrap_or_default();
                }
                "binding.key" => {
                    if let Ok(k) = capture.node.utf8_text(source) {
                        keys.push(k.to_string());
                    }
                }
                _ => {}
            }
        }

        if !target.is_empty() && !keys.is_empty() {
            results.push((target, keys));
        }
    }

    results
}

pub(super) fn extract_data_refs(source: &[u8], root: &tree_sitter::Node) -> Vec<String> {
    let mut refs = Vec::new();
    let mut cursor = QueryCursor::new();
    let key_idx = DATA_REF_QUERY.capture_index_for_name("key").unwrap();
    let val_idx = DATA_REF_QUERY.capture_index_for_name("val").unwrap();
    let mut matches = cursor.matches(&DATA_REF_QUERY, *root, source);

    while let Some(m) = matches.next() {
        let mut key = None;
        let mut val = None;
        for capture in m.captures {
            if capture.index == key_idx {
                key = capture
                    .node
                    .utf8_text(source)
                    .ok()
                    .map(|s: &str| s.to_string());
            } else if capture.index == val_idx {
                val = capture
                    .node
                    .utf8_text(source)
                    .ok()
                    .map(|s: &str| s.to_string());
            }
        }
        if let (Some(k), Some(v)) = (key, val) {
            if k == "data" {
                refs.push(v);
            }
        }
    }

    refs
}

pub(super) fn extract_module_names(source: &[u8], root: &tree_sitter::Node) -> Vec<String> {
    let mut names = Vec::new();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&MODULE_DECL_QUERY, *root, source);

    while let Some(m) = matches.next() {
        for capture in m.captures {
            if let Ok(name) = capture.node.utf8_text(source) {
                names.push(name.trim_matches('"').to_string());
            }
        }
    }

    names
}

/// Collect `(module_name, range)` pairs for `use "Name"` statements. The
/// tree-sitter grammar does not model `use` statements yet, so references are
/// gathered textually; callers gate this behind a clean parse.
pub(super) fn extract_module_uses(source: &str) -> Vec<(String, Range)> {
    fn is_ident_byte(b: u8) -> bool {
        b.is_ascii_alphanumeric() || b == b'_'
    }

    let mut uses = Vec::new();
    for (line_idx, line) in source.lines().enumerate() {
        let code = line.split_once("//").map_or(line, |(c, _)| c);
        let bytes = code.as_bytes();
        let mut i = 0;
        while i + 3 <= bytes.len() {
            if code[i..].starts_with("use")
                && (i == 0 || !is_ident_byte(bytes[i - 1]))
                && (i + 3 >= bytes.len() || !is_ident_byte(bytes[i + 3]))
            {
                let mut j = i + 3;
                while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b'"' {
                    if let Some(len) = code[j + 1..].find('"') {
                        let name = code[j + 1..j + 1 + len].to_string();
                        let end_col = j + 2 + len;
                        uses.push((
                            name,
                            Range::new(
                                Position::new(line_idx as u32, i as u32),
                                Position::new(line_idx as u32, end_col as u32),
                            ),
                        ));
                        i = end_col;
                        continue;
                    }
                }
            }
            i += 1;
        }
    }
    uses
}

/// True for ERROR nodes that represent an `import "x";` statement. The
/// tree-sitter grammar does not model imports yet, so each import line
/// surfaces as an ERROR node; these are tolerated (not reported, and they
/// do not disable semantic checks) because imports are valid DSL syntax.
fn is_import_error(node: &tree_sitter::Node, source: &[u8]) -> bool {
    if !node.is_error() || node.is_missing() {
        return false;
    }
    let text = node.utf8_text(source).unwrap_or("").trim_start();
    text == "import"
        || (text.starts_with("import") && text["import".len()..].starts_with([' ', '\t', '"']))
}

/// True when the tree has ERROR/MISSING nodes outside `import` statements.
/// Import lines are tolerated because the tree-sitter grammar does not model
/// them; every other error keeps semantic checks off.
pub(super) fn has_error_outside_imports(root: &tree_sitter::Node, source: &[u8]) -> bool {
    fn visit(node: &tree_sitter::Node, source: &[u8]) -> bool {
        if (node.is_error() || node.is_missing()) && !is_import_error(node, source) {
            return true;
        }
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                if visit(&cursor.node(), source) {
                    return true;
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
        false
    }

    visit(root, source)
}

/// True when the tree has any ERROR/MISSING node (no import tolerance —
/// used by the mox handlers, whose grammar models imports).
pub(crate) fn has_any_error(root: &tree_sitter::Node, _source: &[u8]) -> bool {
    fn visit(node: &tree_sitter::Node) -> bool {
        if node.is_error() || node.is_missing() {
            return true;
        }
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                if visit(&cursor.node()) {
                    return true;
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
        false
    }

    visit(root)
}

/// Collect the raw paths of top-level `import "x";` statements. The
/// tree-sitter grammar does not model imports yet, so they are gathered
/// textually; callers gate this behind a clean-except-imports parse.
pub(super) fn extract_import_paths(source: &str) -> Vec<String> {
    source
        .lines()
        .filter_map(|line| {
            let rest = line.trim_start().strip_prefix("import ")?.trim();
            let rest = rest.trim_end_matches(';').trim();
            let path = rest.strip_prefix('"')?.strip_suffix('"')?;
            (!path.is_empty()).then(|| path.to_string())
        })
        .collect()
}

/// `(identifier, range)` pairs collected from an IFML array literal.
type ArrayRefs = Vec<(String, Range)>;

/// Collect `(name, range)` pairs for the identifier elements of the
/// `roles: [...]` and `requires: [...]` arrays declared directly in view
/// bodies. Returns `(roles, requires)`.
pub(super) fn extract_view_policy_refs(
    source: &[u8],
    root: &tree_sitter::Node,
) -> (ArrayRefs, ArrayRefs) {
    let mut roles = Vec::new();
    let mut requires = Vec::new();
    let mut stack = vec![*root];
    while let Some(node) = stack.pop() {
        if node.kind() == "view_declaration" {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() != "view_body" {
                    continue;
                }
                let mut body_cursor = child.walk();
                for prop in child.children(&mut body_cursor) {
                    if prop.kind() != "property_assignment" {
                        continue;
                    }
                    let Some(key) = prop.child_by_field_name("key") else {
                        continue;
                    };
                    let Ok(key_text) = key.utf8_text(source) else {
                        continue;
                    };
                    if key_text != "roles" && key_text != "requires" {
                        continue;
                    }
                    let Some(value) = prop.child_by_field_name("value") else {
                        continue;
                    };
                    if key_text == "roles" {
                        collect_array_field_refs(&value, source, &mut roles);
                    } else {
                        collect_array_field_refs(&value, source, &mut requires);
                    }
                }
            }
        }
        let mut walk = node.walk();
        for descendant in node.children(&mut walk) {
            stack.push(descendant);
        }
    }
    (roles, requires)
}

/// Walk the Tree-sitter tree and collect all ERROR/MISSING nodes.
/// Each error node gets a diagnostic with its specific location, so the
/// editor shows red underlines exactly where the syntax error occurs.
/// Import statements are skipped: the grammar does not model them yet, but
/// they are valid DSL syntax (see `is_import_error`).
pub(crate) fn collect_errors(
    node: &tree_sitter::Node,
    source: &[u8],
    diagnostics: &mut Vec<Diagnostic>,
) {
    if (node.is_error() || node.is_missing()) && !is_import_error(node, source) {
        let range = node.range();
        let text = node.utf8_text(source).unwrap_or("<binary>");
        let message = if node.is_missing() {
            format!(
                "Missing syntax element (expected something before '{}')",
                text
            )
        } else {
            format!("Unexpected syntax: '{}'", text)
        };
        diagnostics.push(Diagnostic {
            range: Range::new(
                Position::new(
                    range.start_point.row as u32,
                    range.start_point.column as u32,
                ),
                Position::new(range.end_point.row as u32, range.end_point.column as u32),
            ),
            severity: Some(DiagnosticSeverity::ERROR),
            message,
            source: Some("codegraph".to_string()),
            ..Default::default()
        });
    }
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            collect_errors(&cursor.node(), source, diagnostics);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Collect `(field_name, range)` pairs for identifier elements of the
/// array literal inside a `fields: [...]` value node.
pub(super) fn collect_array_field_refs(
    value_node: &tree_sitter::Node,
    source_bytes: &[u8],
    out: &mut Vec<(String, Range)>,
) {
    let mut cursor = value_node.walk();
    for child in value_node.children(&mut cursor) {
        if child.kind() != "array_literal" {
            continue;
        }
        let mut arr = child.walk();
        for elem in child.children(&mut arr) {
            if elem.kind() != "value_expression" {
                continue;
            }
            let text = match elem.utf8_text(source_bytes) {
                Ok(t) => t.trim().to_string(),
                Err(_) => continue,
            };
            if text.is_empty() || text.contains('"') {
                continue;
            }
            let r = elem.range();
            out.push((
                text,
                Range::new(
                    Position::new(r.start_point.row as u32, r.start_point.column as u32),
                    Position::new(r.end_point.row as u32, r.end_point.column as u32),
                ),
            ));
        }
    }
}

pub(super) fn find_line_with_text(text: &str, needle: &str) -> Option<u32> {
    for (i, line) in text.lines().enumerate() {
        if line.contains(needle) {
            return Some(i as u32);
        }
    }
    None
}
