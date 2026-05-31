use codegraph_ifml_dsl::*;
use lsp_types::*;

use super::state::LspBackend;

pub fn handle_completion(
    backend: &LspBackend,
    params: &CompletionParams,
) -> Option<CompletionResponse> {
    let uri = &params.text_document_position.text_document.uri;
    let text = backend.get_document(uri)?;
    let position = params.text_document_position.position;

    let lines: Vec<&str> = text.lines().collect();
    let current_line = lines.get(position.line as usize)?;
    let before_cursor = &current_line[..position.character as usize];

    let mut items = Vec::new();

    let trimmed = before_cursor.trim();

    if trimmed.ends_with(":") || trimmed.ends_with(": ") {
        let prefix = trimmed.trim_end_matches(':').trim_end_matches(' ').trim();
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
                for name in &backend.entity_names {
                    let detail = backend
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
                if backend.entity_names.is_empty() {
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
            "fields" => {
                items.push(CompletionItem {
                    label: "[".to_string(),
                    kind: Some(CompletionItemKind::SNIPPET),
                    detail: Some("Start field list".to_string()),
                    ..Default::default()
                });
                if let Some(entity_name) = find_current_entity(text, position) {
                    if let Some(info) = backend.schema_infos.get(&entity_name) {
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
            }
            _ => {}
        }
    } else if trimmed.contains("on ") {
        for e in &["select", "submit", "click", "change", "load", "save", "cancel", "delete", "confirm", "back"] {
            items.push(CompletionItem {
                label: e.to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                ..Default::default()
            });
        }
    }

    // navigate("…") — suggest view names from current document
    if items.is_empty() && before_cursor.contains("navigate(\"") {
        if let Ok(model) = parse_ifml(text) {
            for view in &model.views {
                items.push(CompletionItem {
                    label: view.name.clone(),
                    kind: Some(CompletionItemKind::REFERENCE),
                    detail: Some("View".to_string()),
                    ..Default::default()
                });
            }
        }
    }

    // fields: […] — suggest property names
    if items.is_empty() && before_cursor.contains("fields: [") {
        let after_bracket = before_cursor.split("fields: [").last().unwrap_or("");
        if !after_bracket.contains(']') {
            if let Some(entity_name) = find_current_entity(text, position) {
                if let Some(info) = backend.schema_infos.get(&entity_name) {
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
        }
    }

    // params { … } — suggest type names
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
            if text.contains('{') {
                for prop in &["type:", "data:", "fields:", "mode:", "filter:", "sort:", "label:", "landmark:", "xor:", "default:"] {
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
        None
    } else {
        Some(CompletionResponse::List(CompletionList {
            is_incomplete: false,
            items,
        }))
    }
}

/// Scan backwards from the cursor position to find the entity name
/// set via `data:` inside the current component block.
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

pub fn handle_hover(backend: &LspBackend, params: &HoverParams) -> Option<Hover> {
    let uri = &params.text_document_position_params.text_document.uri;
    let text = backend.get_document(uri)?;
    let position = params.text_document_position_params.position;

    let lines: Vec<&str> = text.lines().collect();
    let line = lines.get(position.line as usize)?;
    let word = get_word_at_position(line, position.character as usize)?;

    // Check if the word is an entity name
    if let Some(info) = backend.schema_infos.get(&word) {
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

    // Check if the word is a view name
    if let Ok(model) = parse_ifml(text) {
        if model.views.iter().any(|v| v.name == word) {
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!("**View: {}**\n\nA view container in the IFML model.", word),
                }),
                range: None,
            });
        }
    }

    None
}

pub fn handle_goto_definition(
    backend: &LspBackend,
    params: &GotoDefinitionParams,
) -> Option<GotoDefinitionResponse> {
    let uri = &params.text_document_position_params.text_document.uri;
    let text = backend.get_document(uri)?;
    let position = params.text_document_position_params.position;

    let lines: Vec<&str> = text.lines().collect();
    let line = lines.get(position.line as usize)?;
    let word = get_word_at_position(line, position.character as usize)?;

    // Check if the word is an entity name — navigate to JSON Schema file
    if let Some(info) = backend.schema_infos.get(&word) {
        let rel_path = &info.rel_path;
        for schema_dir in &backend.schema_dirs {
            let full_path = schema_dir.join(rel_path);
            if full_path.exists() {
                let uri_str = format!("file://{}", full_path.display());
                if let Ok(file_uri) = uri_str.parse::<lsp_types::Uri>() {
                    return Some(GotoDefinitionResponse::Scalar(Location {
                        uri: file_uri,
                        range: Range::new(Position::new(0, 0), Position::new(0, 1)),
                    }));
                }
            }
        }
    }

    // Check if the word is a view name — navigate to its declaration in current file
    if let Ok(model) = parse_ifml(text) {
        if let Some(_view) = model.views.iter().find(|v| v.name == word) {
            let view_decl = format!("view \"{}\"", word);
            for (i, line_text) in lines.iter().enumerate() {
                if line_text.contains(&view_decl) {
                    return Some(GotoDefinitionResponse::Scalar(Location {
                        uri: uri.clone(),
                        range: Range::new(
                            Position::new(i as u32, 0),
                            Position::new(i as u32, line_text.len() as u32),
                        ),
                    }));
                }
            }
        }
    }

    None
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

fn find_line_in_text(text: &str, needle: &str) -> u32 {
    for (i, line) in text.lines().enumerate() {
        if line.contains(needle) {
            return i as u32;
        }
    }
    0
}

fn validate_entity_reference(
    backend: &LspBackend,
    text: &str,
    entity_name: &str,
    items: &mut Vec<Diagnostic>,
) {
    if backend.entity_names.is_empty() {
        return;
    }
    if !backend.entity_names.iter().any(|n| n == entity_name) {
        let line = find_line_in_text(text, &format!("data: {entity_name}"));
        items.push(Diagnostic {
            range: Range::new(Position::new(line, 0), Position::new(line, 50)),
            severity: Some(DiagnosticSeverity::ERROR),
            message: format!("Entity '{entity_name}' not found in loaded schemas"),
            source: Some("codegraph".to_string()),
            ..Default::default()
        });
    }
}

fn validate_field_references(
    backend: &LspBackend,
    text: &str,
    entity_name: &str,
    field_names: &[ValueExpression],
    items: &mut Vec<Diagnostic>,
) {
    let Some(props) = backend.schema_infos.get(entity_name).map(|info| &info.properties) else {
        return;
    };

    for field_val in field_names {
        let ValueExpression::Identifier(field_name) = field_val else {
            continue;
        };
        if !props.iter().any(|p| p == field_name) {
            let line = find_line_in_text(text, field_name);
            items.push(Diagnostic {
                range: Range::new(Position::new(line, 0), Position::new(line, 50)),
                severity: Some(DiagnosticSeverity::ERROR),
                message: format!("Field '{field_name}' not found on entity '{entity_name}'"),
                source: Some("codegraph".to_string()),
                ..Default::default()
            });
        }
    }
}

fn validate_navigation_target(
    text: &str,
    target: &str,
    view_names: &[&str],
    items: &mut Vec<Diagnostic>,
) {
    if !view_names.contains(&target) {
        let line = find_line_in_text(text, target);
        items.push(Diagnostic {
            range: Range::new(Position::new(line, 0), Position::new(line, 50)),
            severity: Some(DiagnosticSeverity::WARNING),
            message: format!("View '{target}' not declared"),
            source: Some("codegraph".to_string()),
            ..Default::default()
        });
    }
}

fn validate_event(
    text: &str,
    event: &EventHandler,
    view_names: &[&str],
    items: &mut Vec<Diagnostic>,
) {
    match &event.action {
        EventAction::Navigate { target, .. } => {
            validate_navigation_target(text, target, view_names, items);
        }
        EventAction::Refresh { target, .. } => {
            let line = find_line_in_text(text, target);
            items.push(Diagnostic {
                range: Range::new(Position::new(line, 0), Position::new(line, 50)),
                severity: Some(DiagnosticSeverity::INFORMATION),
                message: format!(
                    "Refresh target '{target}' — verify component exists in current view"
                ),
                source: Some("codegraph".to_string()),
                ..Default::default()
            });
        }
        EventAction::ActionInvocation { name: _, body } => {
            if let Some(body) = body {
                for body_event in &body.handlers {
                    validate_event(text, body_event, view_names, items);
                }
            }
        }
        EventAction::Stay => {}
    }
}

fn validate_component(
    backend: &LspBackend,
    text: &str,
    comp: &ComponentDeclaration,
    view_names: &[&str],
    items: &mut Vec<Diagnostic>,
) {
    let entity_name = comp
        .properties
        .iter()
        .find(|p| p.key == "data")
        .and_then(|p| match &p.value {
            ValueExpression::Identifier(name) => Some(name.as_str()),
            ValueExpression::String(name) => Some(name.as_str()),
            _ => None,
        });

    if let Some(entity_name) = entity_name {
        validate_entity_reference(backend, text, entity_name, items);

        let fields = comp
            .properties
            .iter()
            .find(|p| p.key == "fields")
            .and_then(|p| match &p.value {
                ValueExpression::Array(items) => Some(items),
                _ => None,
            });

        if let Some(fields) = fields {
            validate_field_references(backend, text, entity_name, fields, items);
        }
    }

    for event in &comp.events {
        validate_event(text, event, view_names, items);
    }
}

pub fn handle_diagnostic(
    backend: &LspBackend,
    params: &DocumentDiagnosticParams,
) -> DocumentDiagnosticReport {
    let uri = &params.text_document.uri;
    let text = match backend.get_document(uri) {
        Some(t) => t,
        None => {
            return DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
                related_documents: None,
                full_document_diagnostic_report: FullDocumentDiagnosticReport {
                    result_id: None,
                    items: vec![Diagnostic {
                        range: Range::new(Position::new(0, 0), Position::new(0, 1)),
                        severity: Some(DiagnosticSeverity::ERROR),
                        message: "Document not found in workspace".to_string(),
                        ..Default::default()
                    }],
                },
            });
        }
    };

    let model = match parse_ifml(text) {
        Ok(m) => m,
        Err(parse_err) => {
            return DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
                related_documents: None,
                full_document_diagnostic_report: FullDocumentDiagnosticReport {
                    result_id: None,
                    items: vec![Diagnostic {
                        range: Range::new(Position::new(0, 0), Position::new(0, 1)),
                        severity: Some(DiagnosticSeverity::ERROR),
                        message: format!("Parse error: {parse_err}"),
                        source: Some("codegraph".to_string()),
                        ..Default::default()
                    }],
                },
            });
        }
    };

    let mut items = Vec::new();
    let view_names: Vec<&str> = model.views.iter().map(|v| v.name.as_str()).collect();

    for view in &model.views {
        for comp in &view.components {
            validate_component(backend, text, comp, &view_names, &mut items);
        }
        for event in &view.events {
            validate_event(text, event, &view_names, &mut items);
        }
        for container in &view.containers {
            for comp in &container.components {
                validate_component(backend, text, comp, &view_names, &mut items);
            }
            for event in &container.events {
                validate_event(text, event, &view_names, &mut items);
            }
        }
    }

    for module in &model.modules {
        for comp in &module.components {
            validate_component(backend, text, comp, &view_names, &mut items);
        }
        for event in &module.events {
            validate_event(text, event, &view_names, &mut items);
        }
        for container in &module.containers {
            for comp in &container.components {
                validate_component(backend, text, comp, &view_names, &mut items);
            }
            for event in &container.events {
                validate_event(text, event, &view_names, &mut items);
            }
        }
    }

    for view in &model.views {
        let all_events: Vec<&EventHandler> = view
            .components
            .iter()
            .flat_map(|c| &c.events)
            .chain(&view.events)
            .collect();

        for event in all_events {
            if let EventAction::ActionInvocation { name, body } = &event.action {
                let action_exists = model.actions.iter().any(|a| a.name == *name);
                if !action_exists {
                    let line = find_line_in_text(text, name);
                    items.push(Diagnostic {
                        range: Range::new(Position::new(line, 0), Position::new(line, 50)),
                        severity: Some(DiagnosticSeverity::WARNING),
                        message: format!("Action '{name}' not declared"),
                        source: Some("codegraph".to_string()),
                        ..Default::default()
                    });
                }
                if let Some(body) = body {
                    for body_event in &body.handlers {
                        validate_event(text, body_event, &view_names, &mut items);
                    }
                }
            }
        }
    }

    DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
        related_documents: None,
        full_document_diagnostic_report: FullDocumentDiagnosticReport {
            result_id: None,
            items,
        },
    })
}
