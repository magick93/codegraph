use auto_lsp::anyhow;
use auto_lsp::default::db::{BaseDatabase, BaseDb};
use auto_lsp::lsp_types::*;
use auto_lsp::tree_sitter;

use crate::lsp::mox;

use super::tree_utils::{extract_module_names, extract_view_names, with_grafe};
/// Statement snippets offered inside component bodies (typed component taxonomy).
const COMPONENT_STATEMENT_SNIPPETS: &[(&str, &str, &str)] = &[
    (
        "column",
        "column \"${1:Label}\" -> field ${2:Entity}.${3:property};",
        "Typed column bound to an entity property",
    ),
    (
        "column lookup",
        "column \"${1:Label}\" -> lookup ${2:Entity}.${3:property} via ${4:map};",
        "Typed column with a codelist lookup mapping",
    ),
    (
        "column expr",
        "column \"${1:Label}\" -> expr ${2:expression};",
        "Computed column expression",
    ),
    (
        "field",
        "field ${1:name} -> input ${2|text,textarea,password,email,number,date,time,datetime,dropdown,radio,checkbox,toggle,file,hidden|};",
        "Input field declaration",
    ),
    (
        "chart",
        "chart ${1|bar,line,pie,radar,metric|};",
        "Chart declaration",
    ),
    (
        "chart body",
        "chart ${1|bar,line,pie,radar,metric|} {\n\tlabel: ${2:region};\n\tvalues: [${3:revenue}];\n}",
        "Chart declaration with label and values",
    ),
];

pub fn handle_completion(
    db: &BaseDb,
    params: CompletionParams,
) -> anyhow::Result<Option<CompletionResponse>> {
    let uri = &params.text_document_position.text_document.uri;
    if uri.path().ends_with(".mox") {
        return mox::handle_mox_completion(db, params);
    }
    let position = params.text_document_position.position;

    let file = db
        .get_file(uri)
        .ok_or_else(|| anyhow::anyhow!("File not found"))?;
    let document = file.document(db);
    let source = document.as_str();
    let source_bytes = source.as_bytes();
    let root = document.tree.root_node();
    let lines: Vec<&str> = source.lines().collect();
    let current_line = lines.get(position.line as usize).unwrap_or(&"");
    let before_cursor = &current_line[..position.character as usize];

    let mut items = Vec::new();
    let trimmed = before_cursor.trim();

    if trimmed.ends_with(":") || trimmed.ends_with(": ") {
        let prefix = before_cursor
            .trim_end_matches([' ', '\t'])
            .trim_end_matches(':')
            .split_whitespace()
            .last()
            .unwrap_or("");
        match prefix {
            "type" => {
                for t in &["list", "form", "details", "search", "tree", "chart"] {
                    items.push(CompletionItem {
                        label: t.to_string(),
                        kind: Some(CompletionItemKind::KEYWORD),
                        ..Default::default()
                    });
                }
            }
            "mode" => {
                for m in &["view", "edit", "create"] {
                    items.push(CompletionItem {
                        label: m.to_string(),
                        kind: Some(CompletionItemKind::KEYWORD),
                        ..Default::default()
                    });
                }
            }
            "data" => {
                with_grafe(|grafe| {
                    if let Some(grafe) = grafe {
                        for name in &grafe.entity_names {
                            let detail = grafe
                                .schema_infos
                                .get(name)
                                .map(|s| {
                                    s.description
                                        .clone()
                                        .unwrap_or_else(|| format!("Entity from {}", s.rel_path))
                                })
                                .or_else(|| Some("Entity".to_string()));
                            items.push(CompletionItem {
                                label: name.clone(),
                                kind: Some(CompletionItemKind::CLASS),
                                detail,
                                ..Default::default()
                            });
                        }
                        if grafe.entity_names.is_empty() {
                            items.push(CompletionItem {
                                label: "Customer".to_string(),
                                detail: Some("Example entity".to_string()),
                                ..Default::default()
                            });
                            items.push(CompletionItem {
                                label: "Order".to_string(),
                                detail: Some("Example entity".to_string()),
                                ..Default::default()
                            });
                        }
                    }
                });
            }
            "fields" => {
                if let Some(entity_name) = find_current_entity_ts(source_bytes, &root, position) {
                    with_grafe(|grafe| {
                        if let Some(grafe) = grafe {
                            if let Some(info) = grafe.schema_infos.get(&entity_name) {
                                for prop in &info.properties {
                                    items.push(CompletionItem {
                                        label: prop.clone(),
                                        kind: Some(CompletionItemKind::PROPERTY),
                                        detail: Some(format!("Property of {}", entity_name)),
                                        ..Default::default()
                                    });
                                }
                            }
                        }
                    });
                }
            }
            _ => {}
        }
    } else if trimmed.contains("on ") {
        for e in &[
            "select", "submit", "click", "change", "load", "save", "cancel", "delete", "confirm",
            "back",
        ] {
            items.push(CompletionItem {
                label: e.to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                ..Default::default()
            });
        }
    }

    if items.is_empty() && before_cursor.contains("navigate(\"") {
        let view_names = extract_view_names(source_bytes, &root);
        for name in view_names {
            items.push(CompletionItem {
                label: name,
                kind: Some(CompletionItemKind::REFERENCE),
                detail: Some("View".to_string()),
                ..Default::default()
            });
        }
    }

    if items.is_empty() && before_cursor.contains("use \"") {
        for name in extract_module_names(source_bytes, &root) {
            items.push(CompletionItem {
                label: name,
                kind: Some(CompletionItemKind::MODULE),
                detail: Some("Module".to_string()),
                ..Default::default()
            });
        }
    }

    if items.is_empty() && before_cursor.contains("fields: [") {
        let after_bracket = before_cursor.split("fields: [").last().unwrap_or("");
        if !after_bracket.contains(']') {
            if let Some(entity_name) = find_current_entity_ts(source_bytes, &root, position) {
                with_grafe(|grafe| {
                    if let Some(grafe) = grafe {
                        if let Some(info) = grafe.schema_infos.get(&entity_name) {
                            for prop in &info.properties {
                                items.push(CompletionItem {
                                    label: prop.clone(),
                                    kind: Some(CompletionItemKind::PROPERTY),
                                    detail: Some(format!("Property of {}", entity_name)),
                                    ..Default::default()
                                });
                            }
                        }
                    }
                });
            }
        }
    }

    if items.is_empty() {
        let line_text = lines.get(position.line as usize).unwrap_or(&"");
        if line_text.contains("params {") || before_cursor.contains("params {") {
            let after_open = before_cursor.split("params {").last().unwrap_or("");
            if !after_open.contains('}') {
                for type_name in &["Uuid", "String", "Int", "Float", "Boolean", "DateTime"] {
                    items.push(CompletionItem {
                        label: type_name.to_string(),
                        kind: Some(CompletionItemKind::KEYWORD),
                        detail: Some("Type".to_string()),
                        ..Default::default()
                    });
                }
            }
        }
    }

    if items.is_empty() && (trimmed.is_empty() || trimmed.starts_with("//")) && source.contains('{')
    {
        for prop in &[
            "type:",
            "data:",
            "fields:",
            "mode:",
            "filter:",
            "sort:",
            "label:",
            "landmark:",
            "xor:",
            "default:",
        ] {
            items.push(CompletionItem {
                label: prop.to_string(),
                kind: Some(CompletionItemKind::PROPERTY),
                ..Default::default()
            });
        }

        if in_view_body(&root, position) {
            for (label, detail) in &[
                ("if", "Conditional guard: if <expression>;"),
                (
                    "use",
                    "Module instantiation: use \"Module\" as alias { ... };",
                ),
                ("roles", "Required roles: roles: [role, ...];"),
            ] {
                items.push(CompletionItem {
                    label: (*label).to_string(),
                    kind: Some(CompletionItemKind::KEYWORD),
                    detail: Some((*detail).to_string()),
                    ..Default::default()
                });
            }
        }

        if in_component_body(&root, position) {
            for (label, insert_text, detail) in COMPONENT_STATEMENT_SNIPPETS {
                items.push(CompletionItem {
                    label: (*label).to_string(),
                    kind: Some(CompletionItemKind::SNIPPET),
                    insert_text: Some((*insert_text).to_string()),
                    detail: Some((*detail).to_string()),
                    ..Default::default()
                });
            }
        }
    }

    if items.is_empty() {
        Ok(None)
    } else {
        Ok(Some(CompletionResponse::List(CompletionList {
            is_incomplete: false,
            items,
        })))
    }
}

/// True when the position sits inside a component body (typed statements
/// like column/field/chart are only valid there).
fn in_component_body(root: &tree_sitter::Node, pos: Position) -> bool {
    let point = tree_sitter::Point {
        row: pos.line as usize,
        column: pos.character as usize,
    };

    let mut node = match root.descendant_for_point_range(point, point) {
        Some(n) => n,
        None => return false,
    };

    loop {
        match node.kind() {
            "component_body" => return true,
            "view_body" | "source_file" => return false,
            _ => {}
        }
        match node.parent() {
            Some(parent) => node = parent,
            None => return false,
        }
    }
}

/// True when the position sits at view-body level (not inside a nested
/// component), where view-level keywords like if/use/roles are valid.
fn in_view_body(root: &tree_sitter::Node, pos: Position) -> bool {
    let point = tree_sitter::Point {
        row: pos.line as usize,
        column: pos.character as usize,
    };

    let mut node = match root.descendant_for_point_range(point, point) {
        Some(n) => n,
        None => return false,
    };

    loop {
        match node.kind() {
            "view_body" => return true,
            "component_body" | "source_file" => return false,
            _ => {}
        }
        match node.parent() {
            Some(parent) => node = parent,
            None => return false,
        }
    }
}

fn find_current_entity_ts(
    source_bytes: &[u8],
    root: &tree_sitter::Node,
    pos: Position,
) -> Option<String> {
    let point = tree_sitter::Point {
        row: pos.line as usize,
        column: pos.character as usize,
    };

    let mut node = root.descendant_for_point_range(point, point)?;

    loop {
        match node.kind() {
            "component_body" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() != "property_assignment" {
                        continue;
                    }
                    let key_node = child.child_by_field_name("key")?;
                    if let Ok(key_text) = key_node.utf8_text(source_bytes) {
                        if key_text != "data" {
                            continue;
                        }
                        let value_node = child.child_by_field_name("value")?;
                        return extract_identifier_from_value(source_bytes, &value_node);
                    }
                }
                return None;
            }
            "view_body" | "source_file" => return None,
            _ => {
                node = node.parent()?;
            }
        }
    }
}

pub(super) fn extract_identifier_from_value(
    source_bytes: &[u8],
    value_node: &tree_sitter::Node,
) -> Option<String> {
    let mut cursor = value_node.walk();
    for child in value_node.children(&mut cursor) {
        if child.kind() == "expression" {
            let mut expr_cursor = child.walk();
            for grandchild in child.children(&mut expr_cursor) {
                if grandchild.kind() == "identifier" {
                    return grandchild
                        .utf8_text(source_bytes)
                        .ok()
                        .map(|s| s.to_string());
                }
            }
        }
    }
    None
}
