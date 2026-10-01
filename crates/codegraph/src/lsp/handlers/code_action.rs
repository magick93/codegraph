use std::collections::HashMap;

use auto_lsp::anyhow;
use auto_lsp::default::db::BaseDb;
use auto_lsp::lsp_types::*;

use super::diagnostics::compute_diagnostics;
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
    let field_content = format!(
        r#"    "{}": {{ "type": "string" }},
"#,
        field
    );

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
