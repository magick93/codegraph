use lsp_types::*;

use super::state::LspBackend;
use codegraph_ifml_dsl::parse_ifml;

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
                items.push(CompletionItem {
                    label: "list".to_string(),
                    detail: Some("List component".to_string()),
                    insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                    ..Default::default()
                });
                items.push(CompletionItem {
                    label: "form".to_string(),
                    detail: Some("Form component".to_string()),
                    insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                    ..Default::default()
                });
                items.push(CompletionItem {
                    label: "details".to_string(),
                    detail: Some("Details component".to_string()),
                    insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                    ..Default::default()
                });
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

pub fn handle_hover(_backend: &LspBackend, _params: &HoverParams) -> Option<Hover> {
    None
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
            })
        }
    };

    match parse_ifml(text) {
        Ok(_model) => {
            DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
                related_documents: None,
                full_document_diagnostic_report: FullDocumentDiagnosticReport {
                    result_id: None,
                    items: Vec::new(),
                },
            })
        }
        Err(parse_err) => {
            let error_msg = parse_err.to_string();
            let items = vec![Diagnostic {
                range: Range::new(Position::new(0, 0), Position::new(0, 1)),
                severity: Some(DiagnosticSeverity::ERROR),
                message: error_msg,
                source: Some("codegraph".to_string()),
                ..Default::default()
            }];

            DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
                related_documents: None,
                full_document_diagnostic_report: FullDocumentDiagnosticReport {
                    result_id: None,
                    items,
                },
            })
        }
    }
}
