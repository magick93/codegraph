use std::collections::HashMap;

use auto_lsp::anyhow;
use auto_lsp::default::db::{BaseDatabase, BaseDb};
use auto_lsp::lsp_types::*;
use auto_lsp::tree_sitter;
use auto_lsp::tree_sitter::{QueryCursor, StreamingIterator};
use serde::{Deserialize, Serialize};

use super::tree_utils::VIEW_DECL_QUERY;
// ── ifml/updatePositions (diagram → text layout sync) ──────────────

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePositionsParams {
    pub text_document: TextDocumentIdentifier,
    pub positions: Vec<NodePositionUpdate>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NodePositionUpdate {
    pub name: String,
    pub x: f64,
    pub y: f64,
}

pub enum UpdatePositionsRequest {}

impl auto_lsp::lsp_types::request::Request for UpdatePositionsRequest {
    type Params = UpdatePositionsParams;
    type Result = WorkspaceEdit;
    const METHOD: &'static str = "ifml/updatePositions";
}

fn fmt_coord(n: f64) -> String {
    let s = format!("{:.2}", n);
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

fn offset_to_position(source: &str, offset: usize) -> Position {
    let line = source[..offset.min(source.len())].matches('\n').count() as u32;
    let line_start = source[..offset.min(source.len())]
        .rfind('\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    let column = (offset - line_start) as u32;
    Position::new(line, column)
}

fn node_to_range(source: &str, node: &tree_sitter::Node) -> Range {
    Range::new(
        offset_to_position(source, node.start_byte()),
        offset_to_position(source, node.end_byte()),
    )
}

fn line_indent(source: &str, offset: usize) -> String {
    let line_start = source[..offset.min(source.len())]
        .rfind('\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    let line_end = source[offset.min(source.len())..]
        .find('\n')
        .map(|i| offset + i)
        .unwrap_or(source.len());
    let line = &source[line_start..line_end];
    let ws: String = line
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();
    ws
}

fn find_view_body<'a>(view_decl: &tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    #[allow(clippy::manual_find)] // returning Node<'a> ties to the tree, not the cursor
    {
        let mut cursor = view_decl.walk();
        for child in view_decl.children(&mut cursor) {
            if child.kind() == "view_body" {
                return Some(child);
            }
        }
        None
    }
}

fn find_property_assignment<'a>(
    body: &tree_sitter::Node<'a>,
    source: &[u8],
    key: &str,
) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() != "property_assignment" {
            continue;
        }
        if let Some(key_node) = child.child_by_field_name("key")
            && let Ok(text) = key_node.utf8_text(source)
            && text == key
        {
            return Some(child);
        }
    }
    None
}

fn position_edit_for_view(
    source: &str,
    view_decl: &tree_sitter::Node,
    x: f64,
    y: f64,
) -> Option<TextEdit> {
    let body = find_view_body(view_decl)?;
    let source_bytes = source.as_bytes();
    let new_text = format!("position: {{ x: {}; y: {} }};", fmt_coord(x), fmt_coord(y));

    if let Some(prop) = find_property_assignment(&body, source_bytes, "position") {
        return Some(TextEdit {
            range: node_to_range(source, &prop),
            new_text,
        });
    }

    // MISSING — insert after params/label (pest + tree-sitter both require
    // params_block and label_declaration to come first), else before the
    // first body child, else before the closing brace.
    let mut cursor = body.walk();
    let mut last_prefix: Option<tree_sitter::Node> = None;
    let mut first_child: Option<tree_sitter::Node> = None;
    let mut closing_brace: Option<tree_sitter::Node> = None;
    for child in body.children(&mut cursor) {
        if !child.is_named() {
            if child.kind() == "}" {
                closing_brace = Some(child);
            }
            continue;
        }
        match child.kind() {
            "params_block" | "label_declaration" => last_prefix = Some(child),
            _ => {
                if first_child.is_none() {
                    first_child = Some(child);
                }
            }
        }
    }

    if let Some(anchor) = last_prefix {
        let indent = line_indent(source, anchor.end_byte());
        return Some(TextEdit {
            range: Range::new(
                offset_to_position(source, anchor.end_byte()),
                offset_to_position(source, anchor.end_byte()),
            ),
            new_text: format!("\n{indent}{new_text}"),
        });
    }

    if let Some(child) = first_child {
        let indent = line_indent(source, child.start_byte());
        return Some(TextEdit {
            range: Range::new(
                offset_to_position(source, child.start_byte()),
                offset_to_position(source, child.start_byte()),
            ),
            new_text: format!("{new_text}\n{indent}"),
        });
    }

    if let Some(brace) = closing_brace {
        let indent = line_indent(source, brace.start_byte());
        return Some(TextEdit {
            range: Range::new(
                offset_to_position(source, brace.start_byte()),
                offset_to_position(source, brace.start_byte()),
            ),
            new_text: format!("{indent}{new_text}\n"),
        });
    }

    None
}

pub fn handle_update_positions(
    db: &BaseDb,
    params: UpdatePositionsParams,
) -> anyhow::Result<WorkspaceEdit> {
    let uri = &params.text_document.uri;
    let file = db
        .get_file(uri)
        .ok_or_else(|| anyhow::anyhow!("File not found: {uri}"))?;
    let document = file.document(db);
    let source = document.as_str();
    let root = document.tree.root_node();

    let mut edits = Vec::new();
    for update in &params.positions {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&VIEW_DECL_QUERY, root, source.as_bytes());
        while let Some(m) = matches.next() {
            let mut name_node = None;
            for capture in m.captures {
                if capture.index == 0 {
                    name_node = Some(capture.node);
                    break;
                }
            }
            let Some(name_node) = name_node else { continue };
            let Ok(name) = name_node.utf8_text(source.as_bytes()) else {
                continue;
            };
            let name = name.trim_matches('"');
            if name != update.name {
                continue;
            }
            let Some(parent) = name_node.parent() else {
                continue;
            };
            if let Some(edit) = position_edit_for_view(source, &parent, update.x, update.y) {
                edits.push(edit);
            }
            break;
        }
    }

    let mut changes = HashMap::new();
    if !edits.is_empty() {
        changes.insert(uri.clone(), edits);
    }

    Ok(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    })
}
