use std::sync::LazyLock;

use auto_lsp::anyhow;
use auto_lsp::default::db::{BaseDb, BaseDatabase};
use auto_lsp::lsp_types::*;
use auto_lsp::tree_sitter;
use auto_lsp::tree_sitter::{Query, QueryCursor, StreamingIterator};

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
                if let Some(entity_name) = find_current_entity(source, position) {
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
        let source_bytes = source.as_bytes();
        let root = document.tree.root_node();
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
            if let Some(entity_name) = find_current_entity(source, position) {
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

fn find_current_entity(text: &str, position: Position) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut line_idx = position.line as usize;

    while line_idx > 0 {
        line_idx -= 1;
        let line = lines.get(line_idx)?;
        let trimmed = line.trim();
        if let Some(pos) = trimmed.find("data:") {
            let after = &trimmed[pos + 5..].trim();
            let name = after.split(|c: char| c.is_whitespace() || c == ';').next()?;
            if !name.is_empty() {
                return Some(name.to_string());
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

fn find_line_with_text(text: &str, needle: &str) -> Option<u32> {
    for (i, line) in text.lines().enumerate() {
        if line.contains(needle) {
            return Some(i as u32);
        }
    }
    None
}
