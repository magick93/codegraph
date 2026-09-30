use auto_lsp::anyhow;
use auto_lsp::default::db::{BaseDatabase, BaseDb};
use auto_lsp::lsp_types::*;

use crate::lsp::mox;

use super::tree_utils::{extract_view_names, with_grafe};
pub fn handle_hover(db: &BaseDb, params: HoverParams) -> anyhow::Result<Option<Hover>> {
    let uri = &params.text_document_position_params.text_document.uri;
    if uri.path().ends_with(".mox") {
        return mox::handle_mox_hover(db, params);
    }
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
                    value: format!("**View: {}**\n\nA view container in the IFML model.", word),
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
    if uri.path().ends_with(".mox") {
        // No mox goto-definition in v1.
        return Ok(None);
    }
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

pub(crate) fn get_word_at_position(line: &str, character: usize) -> Option<String> {
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
