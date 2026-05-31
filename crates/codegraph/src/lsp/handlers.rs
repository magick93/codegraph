use std::collections::HashMap;
use std::sync::LazyLock;

use auto_lsp::anyhow;
use auto_lsp::default::db::{BaseDb, BaseDatabase};
use auto_lsp::lsp_types::*;
use auto_lsp::tree_sitter;
use auto_lsp::tree_sitter::{Query, QueryCursor, StreamingIterator};

pub const TOKEN_TYPES: &[&str] = &[
    "namespace", "type", "class", "enumMember", "property", "variable",
    "string", "number", "keyword", "modifier", "event", "operator", "comment",
];

pub const TOKEN_MODIFIERS: &[&str] = &[
    "declaration", "definition", "readonly", "static", "deprecated", "abstract",
];

use super::state::{GrafeoState, GRAFE};

fn with_grafe<F, R>(f: F) -> R
where
    F: FnOnce(Option<&GrafeoState>) -> R,
{
    let guard = GRAFE.get().map(|l| {
        l.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    });
    f(guard.as_ref().and_then(|g| g.as_ref()))
}

static IFML_LANG: LazyLock<tree_sitter::Language> = LazyLock::new(|| {
    tree_sitter_ifml::language()
});

static VIEW_DECL_QUERY: LazyLock<Query> = LazyLock::new(|| {
    Query::new(
        &IFML_LANG,
        r"(view_declaration (string) @view-name)",
    )
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

pub fn handle_completion(
    db: &BaseDb,
    params: CompletionParams,
) -> anyhow::Result<Option<CompletionResponse>> {
    let uri = &params.text_document_position.text_document.uri;
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
            .trim_end_matches(|c: char| c == ' ' || c == '\t')
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
            "select", "submit", "click", "change", "load", "save", "cancel", "delete",
            "confirm", "back",
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

    if items.is_empty() {
        if trimmed.is_empty() || trimmed.starts_with("//") {
            if source.contains('{') {
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

fn extract_identifier_from_value(
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

fn extract_view_names(source: &[u8], root: &tree_sitter::Node) -> Vec<String> {
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

fn extract_views_with_params(source: &[u8], root: &tree_sitter::Node) -> HashMap<String, Vec<String>> {
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
                                                                        if param_child.kind() == "identifier" {
                                                                            if let Ok(name) = param_child.utf8_text(source) {
                                                                                params.push(name.to_string());
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

fn extract_navigate_bindings(source: &[u8], root: &tree_sitter::Node) -> Vec<(String, Vec<String>)> {
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

fn extract_data_refs(source: &[u8], root: &tree_sitter::Node) -> Vec<String> {
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

pub fn handle_hover(
    db: &BaseDb,
    params: HoverParams,
) -> anyhow::Result<Option<Hover>> {
    let uri = &params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let file = db
        .get_file(uri)
        .ok_or_else(|| anyhow::anyhow!("File not found"))?;
    let document = file.document(db);
    let source = document.as_str();

    let lines: Vec<&str> = source.lines().collect();
    let line = lines.get(position.line as usize).unwrap_or(&"");
    let word = get_word_at_position(line, position.character as usize);

    if let Some(word) = word {
        if let Some(hover) = with_grafe(|grafe| {
            if let Some(grafe) = grafe {
                if let Some(info) = grafe.schema_infos.get(&word) {
                    let mut md = format!(
                        "**{}**\n\n{}\n\n",
                        info.title,
                        info.description.as_deref().unwrap_or("No description")
                    );
                    md.push_str("| Field | Type |\n|-------|------|\n");
                    for prop in &info.properties {
                        md.push_str(&format!("| {} | string |\n", prop));
                    }

                    let start_char = position.character.saturating_sub(word.len() as u32);
                    return Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: md,
                        }),
                        range: Some(Range::new(
                            Position::new(position.line, start_char),
                            Position::new(position.line, start_char + word.len() as u32),
                        )),
                    });
                }
            }
            None
        }) {
            return Ok(Some(hover));
        }

        let source_bytes = source.as_bytes();
        let root = document.tree.root_node();
        let view_names = extract_view_names(source_bytes, &root);
        if view_names.iter().any(|v| v == &word) {
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!(
                        "**View: {}**\n\nA view container in the IFML model.",
                        word
                    ),
                }),
                range: None,
            }));
        }
    }

    Ok(None)
}

pub fn handle_goto_definition(
    db: &BaseDb,
    params: GotoDefinitionParams,
) -> anyhow::Result<Option<GotoDefinitionResponse>> {
    let uri = &params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let file = db
        .get_file(uri)
        .ok_or_else(|| anyhow::anyhow!("File not found"))?;
    let document = file.document(db);
    let source = document.as_str();

    let lines: Vec<&str> = source.lines().collect();
    let line = lines.get(position.line as usize).unwrap_or(&"");
    let word = get_word_at_position(line, position.character as usize);

    if let Some(word) = word {
        let mut entity_result = None;
        with_grafe(|grafe| {
            if let Some(grafe) = grafe {
                if let Some(info) = grafe.schema_infos.get(&word) {
                    for schema_dir in &grafe.schema_dirs {
                        let full_path = schema_dir.join(&info.rel_path);
                        if full_path.exists() {
                            let uri_str = format!("file://{}", full_path.display());
                            if let Ok(file_uri) = uri_str.parse::<Url>() {
                                entity_result = Some(GotoDefinitionResponse::Scalar(Location {
                                    uri: file_uri,
                                    range: Range::new(Position::new(0, 0), Position::new(0, 1)),
                                }));
                            }
                        }
                    }
                }
            }
        });
        if let Some(result) = entity_result {
            return Ok(Some(result));
        }

        let source_bytes = source.as_bytes();
        let root = document.tree.root_node();
        let view_names = extract_view_names(source_bytes, &root);
        if view_names.iter().any(|v| v == &word) {
            let view_decl = format!("view \"{}\"", word);
            for (i, line_text) in lines.iter().enumerate() {
                if line_text.contains(&view_decl) {
                    return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                        uri: uri.clone(),
                        range: Range::new(
                            Position::new(i as u32, 0),
                            Position::new(i as u32, line_text.len() as u32),
                        ),
                    })));
                }
            }
        }
    }

    Ok(None)
}

fn get_word_at_position(line: &str, character: usize) -> Option<String> {
    let chars: Vec<char> = line.chars().collect();
    if character >= chars.len() {
        return None;
    }

    let mut start = character;
    let mut end = character;

    while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_') {
        start -= 1;
    }
    while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
        end += 1;
    }

    if start < end {
        Some(chars[start..end].iter().collect())
    } else {
        None
    }
}

pub fn compute_diagnostics(
    db: &BaseDb,
    uri: &Url,
) -> Vec<Diagnostic> {
    let file = match db.get_file(uri) {
        Some(f) => f,
        None => return Vec::new(),
    };
    let document = file.document(db);
    let source = document.as_str();
    let source_bytes = source.as_bytes();

    let mut diagnostics = Vec::new();

    let root = document.tree.root_node();
    if root.has_error() {
        diagnostics.push(Diagnostic {
            range: Range::new(Position::new(0, 0), Position::new(0, 1)),
            severity: Some(DiagnosticSeverity::ERROR),
            message: "Parse error in IFML document".to_string(),
            source: Some("codegraph".to_string()),
            ..Default::default()
        });
    }

    let data_refs = extract_data_refs(source_bytes, &root);
    with_grafe(|grafe| {
        if let Some(grafe) = grafe {
            if !grafe.entity_names.is_empty() {
                for ref_name in &data_refs {
                    if !grafe.entity_names.contains(ref_name) {
                        if let Some(line) = find_line_with_text(source, ref_name) {
                            diagnostics.push(Diagnostic {
                                range: Range::new(
                                    Position::new(line, 0),
                                    Position::new(line, 50),
                                ),
                                severity: Some(DiagnosticSeverity::ERROR),
                                message: format!(
                                    "Entity '{}' not found in loaded schemas",
                                    ref_name
                                ),
                                source: Some("codegraph".to_string()),
                                ..Default::default()
                            });
                        }
                    }
                }
            }
        }
    });

    let views_with_params = extract_views_with_params(source_bytes, &root);
    let navigate_bindings = extract_navigate_bindings(source_bytes, &root);

    for (target, keys) in &navigate_bindings {
        if let Some(expected_params) = views_with_params.get(target.as_str()) {
            for key in keys {
                if !expected_params.contains(key) {
                    if let Some(line) = find_line_with_text(source, key) {
                        diagnostics.push(Diagnostic {
                            range: Range::new(
                                Position::new(line, 0),
                                Position::new(line, 50),
                            ),
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
    }

    diagnostics
}

pub fn handle_semantic_tokens_full(
    db: &BaseDb,
    params: SemanticTokensParams,
) -> anyhow::Result<Option<SemanticTokensResult>> {
    let uri = &params.text_document.uri;
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
        let delta_start = if delta_line == 0 { *col - prev_col } else { *col };
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

        "view" | "component" | "container" | "module" | "domain" | "schema"
        | "on" | "navigate" | "refresh" | "action" | "params" | "label"
        | "stay_statement" | "input" | "output" | "true" | "false" => {
            add_semantic_token(node, tokens, 8, 0);
        }

        "select" | "submit" | "click" | "change" | "load" | "save"
        | "cancel" | "delete" | "confirm" | "back" => {
            add_semantic_token(node, tokens, 10, 0);
        }

        "Boolean" | "DateTime" | "Float" | "Int" | "String" | "Uuid" => {
            add_semantic_token(node, tokens, 1, 0);
        }

        "->" | "==" | "!=" | "!~" | "~=" | "<" | "<=" | ">" | ">="
        | "+" | "-" | "*" | "/" | "%" | "&&" | "||" | "!" => {
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

fn find_line_with_text(text: &str, needle: &str) -> Option<u32> {
    for (i, line) in text.lines().enumerate() {
        if line.contains(needle) {
            return Some(i as u32);
        }
    }
    None
}

pub fn handle_code_action(
    db: &BaseDb,
    params: CodeActionParams,
) -> anyhow::Result<Option<Vec<CodeActionOrCommand>>> {
    let uri = &params.text_document.uri;
    let diagnostics = compute_diagnostics(db, uri);
    let range = params.range;

    let relevant: Vec<&Diagnostic> = diagnostics
        .iter()
        .filter(|d| ranges_overlap(&d.range, &range))
        .collect();

    if relevant.is_empty() {
        return Ok(None);
    }

    let mut actions: Vec<CodeActionOrCommand> = Vec::new();

    for diag in &relevant {
        if diag.message.contains("not found in loaded schemas") {
            if let Some(name) = extract_name_from_msg(&diag.message) {
                actions.push(
                    CodeAction {
                        title: format!("Create schema file for '{}'", name),
                        kind: Some(CodeActionKind::QUICKFIX),
                        is_preferred: None,
                        diagnostics: Some(vec![(*diag).clone()]),
                        edit: Some(create_schema_edit(&name)),
                        command: None,
                        disabled: None,
                        data: None,
                    }
                    .into(),
                );

                actions.push(
                    CodeAction {
                        title: format!("Import '{}' from known domain", name),
                        kind: Some(CodeActionKind::QUICKFIX),
                        is_preferred: None,
                        diagnostics: Some(vec![(*diag).clone()]),
                        edit: None,
                        command: None,
                        disabled: None,
                        data: None,
                    }
                    .into(),
                );
            }
        }

        if diag.message.contains("not found on entity") {
            if let Some((field, entity)) = extract_field_entity_from_msg(&diag.message) {
                actions.push(
                    CodeAction {
                        title: format!("Add field '{}' to '{}' schema", field, entity),
                        kind: Some(CodeActionKind::QUICKFIX),
                        is_preferred: None,
                        diagnostics: Some(vec![(*diag).clone()]),
                        edit: Some(create_field_edit(&entity, &field)),
                        command: None,
                        disabled: None,
                        data: None,
                    }
                    .into(),
                );
            }
        }
    }

    if actions.is_empty() {
        return Ok(None);
    }

    Ok(Some(actions))
}

fn ranges_overlap(a: &Range, b: &Range) -> bool {
    a.start.line <= b.end.line && b.start.line <= a.end.line
}

fn extract_name_from_msg(msg: &str) -> Option<String> {
    msg.split('\'').nth(1).map(|s| s.to_string())
}

fn extract_field_entity_from_msg(msg: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = msg.split('\'').collect();
    if parts.len() >= 4 {
        Some((parts[1].to_string(), parts[3].to_string()))
    } else {
        None
    }
}

fn create_schema_edit(entity_name: &str) -> WorkspaceEdit {
    let schema_content = format!(
        r#"{{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "{}Type",
  "type": "object",
  "description": "Auto-generated schema for {}",
  "properties": {{}},
  "required": []
}}"#,
        entity_name, entity_name
    );

    let uri_str = format!("file:///schemas/{}.json", entity_name.to_lowercase());
    let uri = Url::parse(&uri_str).expect("valid URI");

    let mut changes = HashMap::new();
    changes.insert(
        uri,
        vec![TextEdit {
            range: Range::new(Position::new(0, 0), Position::new(0, 0)),
            new_text: schema_content,
        }],
    );

    WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    }
}

fn create_field_edit(entity: &str, field: &str) -> WorkspaceEdit {
    let field_content = format!(r#"    "{}": {{ "type": "string" }},
"#, field);

    let uri_str = format!("file:///schemas/{}.json", entity.to_lowercase());
    let uri = Url::parse(&uri_str).expect("valid URI");

    let mut changes = HashMap::new();
    changes.insert(
        uri,
        vec![TextEdit {
            range: Range::new(Position::new(0, 0), Position::new(0, 0)),
            new_text: field_content,
        }],
    );

    WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    }
}
