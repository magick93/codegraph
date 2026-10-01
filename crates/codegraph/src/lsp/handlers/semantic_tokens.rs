use auto_lsp::anyhow;
use auto_lsp::default::db::{BaseDatabase, BaseDb};
use auto_lsp::lsp_types::*;
use auto_lsp::tree_sitter;
pub const TOKEN_TYPES: &[&str] = &[
    "namespace",
    "type",
    "class",
    "enumMember",
    "property",
    "variable",
    "string",
    "number",
    "keyword",
    "modifier",
    "event",
    "operator",
    "comment",
];

pub const TOKEN_MODIFIERS: &[&str] = &[
    "declaration",
    "definition",
    "readonly",
    "static",
    "deprecated",
    "abstract",
];

pub fn handle_semantic_tokens_full(
    db: &BaseDb,
    params: SemanticTokensParams,
) -> anyhow::Result<Option<SemanticTokensResult>> {
    let uri = &params.text_document.uri;
    if uri.path().ends_with(".mox") {
        // No mox semantic tokens in v1.
        return Ok(None);
    }
    let file = match db.get_file(uri) {
        Some(f) => f,
        None => return Ok(None),
    };
    let document = file.document(db);
    let source = document.as_str();
    let source_bytes = source.as_bytes();
    let root = document.tree.root_node();

    let mut raw: Vec<(u32, u32, u32, u32, u32)> = Vec::new();
    walk_semantic(&root, source_bytes, &mut raw)?;

    let mut data = Vec::with_capacity(raw.len());
    let mut prev_line = 0u32;
    let mut prev_col = 0u32;
    for (line, col, len, ty, mods) in &raw {
        let delta_line = *line - prev_line;
        let delta_start = if delta_line == 0 {
            *col - prev_col
        } else {
            *col
        };
        data.push(SemanticToken {
            delta_line,
            delta_start,
            length: *len,
            token_type: *ty,
            token_modifiers_bitset: *mods,
        });
        prev_line = *line;
        prev_col = *col;
    }

    Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data,
    })))
}

fn walk_semantic(
    node: &tree_sitter::Node,
    source: &[u8],
    tokens: &mut Vec<(u32, u32, u32, u32, u32)>,
) -> anyhow::Result<()> {
    let kind = node.kind();

    match kind {
        "identifier" => {
            let (ty, mods) = classify_identifier(node, source);
            add_semantic_token(node, tokens, ty, mods);
        }
        "string" => add_semantic_token(node, tokens, 6, 0),
        "number" => add_semantic_token(node, tokens, 7, 0),
        "comment" => add_semantic_token(node, tokens, 12, 0),
        "boolean" => add_semantic_token(node, tokens, 8, 0),

        "view" | "component" | "container" | "module" | "domain" | "schema" | "on" | "navigate"
        | "refresh" | "action" | "params" | "label" | "stay_statement" | "input" | "output"
        | "true" | "false" | "column" | "field" | "chart" | "lookup" | "via" | "expr" | "if"
        | "use" | "as" | "actor" => {
            add_semantic_token(node, tokens, 8, 0);
        }

        "select" | "submit" | "click" | "change" | "load" | "save" | "cancel" | "delete"
        | "confirm" | "back" => {
            add_semantic_token(node, tokens, 10, 0);
        }

        "Boolean" | "DateTime" | "Float" | "Int" | "String" | "Uuid" | "input_type"
        | "chart_kind" => {
            add_semantic_token(node, tokens, 1, 0);
        }

        "->" | "==" | "!=" | "!~" | "~=" | "<" | "<=" | ">" | ">=" | "+" | "-" | "*" | "/"
        | "%" | "&&" | "||" | "!" => {
            add_semantic_token(node, tokens, 11, 0);
        }

        _ => {}
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            walk_semantic(&cursor.node(), source, tokens)?;
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    Ok(())
}

fn add_semantic_token(
    node: &tree_sitter::Node,
    tokens: &mut Vec<(u32, u32, u32, u32, u32)>,
    type_idx: u32,
    modifier_mask: u32,
) {
    let start = node.start_position();
    let end = node.end_position();
    if start.row != end.row {
        return;
    }
    tokens.push((
        start.row as u32,
        start.column as u32,
        (end.column - start.column) as u32,
        type_idx,
        modifier_mask,
    ));
}

fn classify_identifier(node: &tree_sitter::Node, source: &[u8]) -> (u32, u32) {
    let mut current = *node;
    loop {
        let parent = match current.parent() {
            Some(p) => p,
            None => return (5, 0),
        };
        match parent.kind() {
            "property_assignment" => {
                if let Some(key) = parent.child_by_field_name("key") {
                    if key == current {
                        return (4, 0);
                    }
                    if let Ok(key_text) = key.utf8_text(source) {
                        return match key_text {
                            "type" => (3, 0),
                            "data" => (1, 0),
                            _ => (5, 0),
                        };
                    }
                }
                return (5, 0);
            }
            "call_expr" => {
                return (8, 0);
            }
            "event_type" => {
                return (10, 0);
            }
            "type_ref" => {
                return (1, 0);
            }
            "binding_pair" => {
                if let Some(key) = parent.child_by_field_name("key") {
                    if key == current {
                        return (4, 0);
                    }
                }
                return (5, 0);
            }
            "field_expr" => {
                return (5, 0);
            }
            _ => {
                current = parent;
            }
        }
    }
}
