use std::collections::HashSet;
use std::path::Path;
use std::sync::LazyLock;

use auto_lsp::anyhow;
use auto_lsp::default::db::{BaseDatabase, BaseDb};
use auto_lsp::lsp_types::*;
use auto_lsp::tree_sitter;
use auto_lsp::tree_sitter::{Query, QueryCursor, StreamingIterator};

use crate::ifml_actor_import::resolve_actor_policy;
use crate::lsp::mox;
use crate::lsp::state::GrafeoState;

use super::completion::extract_identifier_from_value;
use super::tree_utils::{
    IFML_LANG, collect_array_field_refs, collect_errors, extract_data_refs, extract_import_paths,
    extract_module_names, extract_module_uses, extract_navigate_bindings, extract_view_names,
    extract_view_policy_refs, extract_views_with_params, find_line_with_text,
    has_error_outside_imports, with_grafe,
};
const VALID_COMPONENT_TYPES: &[&str] = &["list", "form", "details", "search", "tree", "chart"];

static NAV_TARGET_QUERY: LazyLock<Query> = LazyLock::new(|| {
    Query::new(&IFML_LANG, r"(navigate_action (string) @target)")
        .expect("Failed to create navigate target query")
});

/// Matches `type: SomeValue` where SomeValue is any identifier
static TYPE_VALUE_QUERY: LazyLock<Query> = LazyLock::new(|| {
    Query::new(
        &IFML_LANG,
        r"(property_assignment
            key: (identifier) @type_key
            value: (value_expression (expression (identifier) @type_val))
        )",
    )
    .expect("Failed to create type value query")
});

pub fn compute_diagnostics(db: &BaseDb, uri: &Url) -> Vec<Diagnostic> {
    if uri.path().ends_with(".mox") {
        return mox::compute_mox_diagnostics(db, uri);
    }
    let file = match db.get_file(uri) {
        Some(f) => f,
        None => return Vec::new(),
    };
    let document = file.document(db);
    let source = document.as_str();
    let source_bytes = source.as_bytes();
    let mut diagnostics = Vec::new();

    let root = document.tree.root_node();
    // Walk the Tree-sitter tree for ERROR/MISSING nodes and report each
    // with its specific location, so the editor shows red underlines
    // exactly where the syntax error occurs.
    collect_errors(&root, source_bytes, &mut diagnostics);

    // Validate component type values (must be list/form/details/search/tree/chart)
    validate_component_types(source_bytes, &root, &mut diagnostics);

    // Validate no duplicate fields in fields: [...] arrays
    validate_no_duplicate_fields(source_bytes, &root, &mut diagnostics);

    // Validate fields: [...] entries against the bound entity's schema.
    // Only runs when schemas are configured.
    with_grafe(|grafe| {
        if let Some(grafe) = grafe
            && !grafe.schema_infos.is_empty()
        {
            validate_fields_against_schema(source, &root, grafe, &mut diagnostics);
        }
    });

    let data_refs = extract_data_refs(source_bytes, &root);
    with_grafe(|grafe| {
        if let Some(grafe) = grafe
            && !grafe.entity_names.is_empty()
        {
            for ref_name in &data_refs {
                if !grafe.entity_names.contains(ref_name)
                    && let Some(line) = find_line_with_text(source, ref_name)
                {
                    diagnostics.push(Diagnostic {
                        range: Range::new(Position::new(line, 0), Position::new(line, 50)),
                        severity: Some(DiagnosticSeverity::ERROR),
                        message: format!("Entity '{}' not found in loaded schemas", ref_name),
                        source: Some("codegraph".to_string()),
                        ..Default::default()
                    });
                }
            }
        }
    });

    let views_with_params = extract_views_with_params(source_bytes, &root);
    let navigate_bindings = extract_navigate_bindings(source_bytes, &root);

    for (target, keys) in &navigate_bindings {
        if let Some(expected_params) = views_with_params.get(target.as_str()) {
            for key in keys {
                if !expected_params.contains(key)
                    && let Some(line) = find_line_with_text(source, key)
                {
                    diagnostics.push(Diagnostic {
                        range: Range::new(Position::new(line, 0), Position::new(line, 50)),
                        severity: Some(DiagnosticSeverity::WARNING),
                        message: format!(
                            "'{}' is not a declared parameter of view '{}'. Expected: {:?}",
                            key, target, expected_params
                        ),
                        source: Some("codegraph".to_string()),
                        ..Default::default()
                    });
                }
            }
        }
    }

    // Semantic reference checks only run on cleanly parsed documents so
    // tree-sitter error recovery cannot fabricate bogus references. Import
    // statements are tolerated: the tree-sitter grammar does not model them
    // yet, so every `import "x";` line surfaces as an ERROR node.
    if !has_error_outside_imports(&root, source_bytes) {
        validate_navigate_targets(source_bytes, &root, &mut diagnostics);
        validate_module_uses(source, &root, &mut diagnostics);
        validate_policy_refs(source, &root, uri, &mut diagnostics);
    }

    diagnostics
}

pub fn handle_document_diagnostic(
    db: &BaseDb,
    params: DocumentDiagnosticParams,
) -> anyhow::Result<DocumentDiagnosticReportResult> {
    let diagnostics = compute_diagnostics(db, &params.text_document.uri);
    Ok(DocumentDiagnosticReportResult::Report(
        DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
            related_documents: None,
            full_document_diagnostic_report: FullDocumentDiagnosticReport {
                result_id: None,
                items: diagnostics,
            },
        }),
    ))
}

/// Warn about `navigate("X", ...)` targets that name no view declared in the
/// same document (multi-file models are out of scope).
fn validate_navigate_targets(
    source: &[u8],
    root: &tree_sitter::Node,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let view_names: HashSet<String> = extract_view_names(source, root).into_iter().collect();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&NAV_TARGET_QUERY, *root, source);

    while let Some(m) = matches.next() {
        for capture in m.captures {
            let Ok(text) = capture.node.utf8_text(source) else {
                continue;
            };
            let target = text.trim_matches('"');
            if target.is_empty() || view_names.contains(target) {
                continue;
            }
            let r = capture.node.range();
            diagnostics.push(Diagnostic {
                range: Range::new(
                    Position::new(r.start_point.row as u32, r.start_point.column as u32),
                    Position::new(r.end_point.row as u32, r.end_point.column as u32),
                ),
                severity: Some(DiagnosticSeverity::WARNING),
                message: format!("Unknown view '{target}'"),
                source: Some("codegraph".to_string()),
                ..Default::default()
            });
        }
    }
}

/// Warn about `use "M"` instantiations naming no `module "M"` declared in
/// the same document.
fn validate_module_uses(source: &str, root: &tree_sitter::Node, diagnostics: &mut Vec<Diagnostic>) {
    let declared: HashSet<String> = extract_module_names(source.as_bytes(), root)
        .into_iter()
        .collect();
    for (name, range) in extract_module_uses(source) {
        if declared.contains(&name) {
            continue;
        }
        diagnostics.push(Diagnostic {
            range,
            severity: Some(DiagnosticSeverity::WARNING),
            message: format!("Unknown module '{name}'"),
            source: Some("codegraph".to_string()),
            ..Default::default()
        });
    }
}

/// Validate view `roles`/`requires` against the actor policy resolved from
/// the document's imports. Quiet unless a policy actually resolves: without
/// one, roles stay undiagnosed (they may be free-form) and unresolvable
/// imports never produce a diagnostics storm.
fn validate_policy_refs(
    source: &str,
    root: &tree_sitter::Node,
    uri: &Url,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let imports = extract_import_paths(source);
    if imports.is_empty() {
        return;
    }
    let Ok(doc_path) = uri.to_file_path() else {
        return;
    };
    let base_dir = doc_path.parent().unwrap_or_else(|| Path::new("."));
    let Some(policy) = resolve_actor_policy(&imports, base_dir) else {
        return;
    };
    let actor_names: HashSet<String> = policy.actors.into_iter().map(|a| a.name).collect();
    let capability_names: HashSet<String> =
        policy.capabilities.into_iter().map(|c| c.name).collect();
    let (roles, requires) = extract_view_policy_refs(source.as_bytes(), root);
    for (name, range) in roles {
        if actor_names.contains(&name) {
            continue;
        }
        diagnostics.push(Diagnostic {
            range,
            severity: Some(DiagnosticSeverity::WARNING),
            message: format!("Unknown actor '{name}'"),
            source: Some("codegraph".to_string()),
            ..Default::default()
        });
    }
    for (name, range) in requires {
        if capability_names.contains(&name) {
            continue;
        }
        diagnostics.push(Diagnostic {
            range,
            severity: Some(DiagnosticSeverity::WARNING),
            message: format!("Unknown capability '{name}'"),
            source: Some("codegraph".to_string()),
            ..Default::default()
        });
    }
}

/// Validate that every `type:` property uses a known component type value.
fn validate_component_types(
    source: &[u8],
    root: &tree_sitter::Node,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&TYPE_VALUE_QUERY, *root, source);
    let name_idx = TYPE_VALUE_QUERY.capture_index_for_name("type_key").unwrap();
    let val_idx = TYPE_VALUE_QUERY.capture_index_for_name("type_val").unwrap();

    while let Some(m) = matches.next() {
        let mut key = None;
        let mut val_text = None;
        let mut val_range = None;
        for capture in m.captures {
            if capture.index == name_idx {
                key = capture
                    .node
                    .utf8_text(source)
                    .ok()
                    .map(|s: &str| s.to_string());
            } else if capture.index == val_idx {
                val_text = capture
                    .node
                    .utf8_text(source)
                    .ok()
                    .map(|s: &str| s.to_string());
                let r = capture.node.range();
                val_range = Some(Range::new(
                    Position::new(r.start_point.row as u32, r.start_point.column as u32),
                    Position::new(r.end_point.row as u32, r.end_point.column as u32),
                ));
            }
        }
        if let (Some(k), Some(v), Some(range)) = (key, val_text, val_range)
            && k == "type"
            && !VALID_COMPONENT_TYPES.contains(&v.as_str())
        {
            diagnostics.push(Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::ERROR),
                message: format!(
                    "Unknown component type '{}'. Expected one of: {}",
                    v,
                    VALID_COMPONENT_TYPES.join(", ")
                ),
                source: Some("codegraph".to_string()),
                ..Default::default()
            });
        }
    }
}

/// Recursively walk a node and its descendants looking for property_assignment nodes.
fn walk_for_fields_assignments(
    node: &tree_sitter::Node,
    source: &[u8],
    diagnostics: &mut Vec<Diagnostic>,
) {
    if node.kind() == "property_assignment" {
        check_fields_duplicates(node, source, diagnostics);
        return; // property_assignment children are not nested property_assignments
    }
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            walk_for_fields_assignments(&cursor.node(), source, diagnostics);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Check a single property_assignment node: if its key is "fields", look for duplicates
/// inside its array_literal value.
fn check_fields_duplicates(
    node: &tree_sitter::Node,
    source: &[u8],
    diagnostics: &mut Vec<Diagnostic>,
) {
    // First child should be the key identifier
    let mut c = node.walk();
    if !c.goto_first_child() {
        return;
    }
    let first = c.node();
    if first.kind() != "identifier" {
        return;
    }
    let key_text = match first.utf8_text(source) {
        Ok(t) => t,
        Err(_) => return,
    };
    if key_text != "fields" {
        return;
    }

    // Walk children to find value_expression → array_literal
    let mut seen: std::collections::HashMap<&str, ()> = std::collections::HashMap::new();

    // Restart from first child of the property_assignment
    let mut walker = node.walk();
    if !walker.goto_first_child() {
        return;
    }
    loop {
        let child = walker.node();
        if child.kind() == "value_expression" {
            // Find array_literal inside this value_expression
            let mut ve = child.walk();
            if ve.goto_first_child() {
                loop {
                    let ve_child = ve.node();
                    if ve_child.kind() == "array_literal" {
                        // Walk array_literal children looking for value_expression elements
                        let mut arr = ve_child.walk();
                        if arr.goto_first_child() {
                            loop {
                                let elem = arr.node();
                                if elem.kind() == "value_expression"
                                    && let Ok(text) = elem.utf8_text(source)
                                {
                                    let trimmed = text.trim();
                                    if seen.contains_key(trimmed) {
                                        let r = elem.range();
                                        diagnostics.push(Diagnostic {
                                            range: Range::new(
                                                Position::new(
                                                    r.start_point.row as u32,
                                                    r.start_point.column as u32,
                                                ),
                                                Position::new(
                                                    r.end_point.row as u32,
                                                    r.end_point.column as u32,
                                                ),
                                            ),
                                            severity: Some(DiagnosticSeverity::WARNING),
                                            message: format!("Duplicate field '{}'", trimmed),
                                            source: Some("codegraph".to_string()),
                                            ..Default::default()
                                        });
                                    } else {
                                        seen.insert(trimmed, ());
                                    }
                                }
                                if !arr.goto_next_sibling() {
                                    break;
                                }
                            }
                        }
                    }
                    if !ve.goto_next_sibling() {
                        break;
                    }
                }
            }
        }
        if !walker.goto_next_sibling() {
            break;
        }
    }
}

/// Validate no duplicate field names within each individual fields: [...] array.
/// Each `fields:` array is checked independently (same field in different views is NOT a duplicate).
fn validate_no_duplicate_fields(
    source: &[u8],
    root: &tree_sitter::Node,
    diagnostics: &mut Vec<Diagnostic>,
) {
    walk_for_fields_assignments(root, source, diagnostics);
}

/// Validate every `fields: [...]` array against the properties of the
/// entity bound via `data:` in the same component. Unknown fields produce a
/// warning naming the schema title. No-ops for components whose entity is
/// not in the loaded schemas (the `data:` check already reports those).
fn validate_fields_against_schema(
    source: &str,
    root: &tree_sitter::Node,
    grafe: &GrafeoState,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let source_bytes = source.as_bytes();
    let mut stack = vec![*root];
    while let Some(node) = stack.pop() {
        if node.kind() == "component_body" {
            check_component_fields(&node, source_bytes, grafe, diagnostics);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
}

fn check_component_fields(
    body: &tree_sitter::Node,
    source_bytes: &[u8],
    grafe: &GrafeoState,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut entity = None;
    let mut field_refs = Vec::new();
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() != "property_assignment" {
            continue;
        }
        let key = match child.child_by_field_name("key") {
            Some(k) => match k.utf8_text(source_bytes) {
                Ok(t) => t.to_string(),
                Err(_) => continue,
            },
            None => continue,
        };
        let value = match child.child_by_field_name("value") {
            Some(v) => v,
            None => continue,
        };
        match key.as_str() {
            "data" => entity = extract_identifier_from_value(source_bytes, &value),
            "fields" => collect_array_field_refs(&value, source_bytes, &mut field_refs),
            _ => {}
        }
    }

    let Some(entity) = entity else { return };
    let Some(info) = grafe.schema_infos.get(&entity) else {
        return;
    };
    for (name, range) in field_refs {
        if !info.properties.contains(&name) {
            diagnostics.push(Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::WARNING),
                message: format!(
                    "Field '{}' not found on entity '{}' (schema '{}')",
                    name, entity, info.title
                ),
                source: Some("codegraph".to_string()),
                ..Default::default()
            });
        }
    }
}
