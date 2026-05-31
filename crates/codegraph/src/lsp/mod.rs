use lsp_server::{Connection, Message, Notification, Request, Response};
use lsp_types::*;

use crate::error::{Error, Result};

mod handlers;
mod state;
#[cfg(test)]
mod tests;

pub use state::{LspBackend, SchemaInfo};

/// Run the LSP server loop with the given connection and backend state.
pub fn run_lsp_server(connection: Connection, backend: LspBackend) -> Result<()> {
    let capabilities = serde_json::to_value(server_capabilities())
        .map_err(|e| Error::Config(format!("Failed to serialize capabilities: {e}")))?;

    let _init_params = connection
        .initialize(capabilities)
        .map_err(|e| Error::Config(format!("Initialize failed: {e}")))?;

    main_loop(&connection, backend)?;

    Ok(())
}

fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec![":".to_string(), "\"".to_string(), ".".to_string()]),
            ..Default::default()
        }),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        diagnostic_provider: Some(DiagnosticServerCapabilities::Options(DiagnosticOptions {
            identifier: None,
            inter_file_dependencies: true,
            workspace_diagnostics: false,
            work_done_progress_options: WorkDoneProgressOptions {
                work_done_progress: None,
            },
        })),
        document_symbol_provider: Some(OneOf::Left(true)),
        definition_provider: Some(OneOf::Left(true)),
        ..Default::default()
    }
}

fn main_loop(connection: &Connection, mut backend: LspBackend) -> Result<()> {
    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection
                    .handle_shutdown(&req)
                    .map_err(|e| Error::Config(format!("Shutdown error: {e}")))?
                {
                    return Ok(());
                }
                handle_request(connection, &mut backend, req)?;
            }
            Message::Notification(not) => {
                handle_notification(&mut backend, not)?;
            }
            Message::Response(_) => {}
        }
    }
    Ok(())
}

fn handle_request(
    connection: &Connection,
    backend: &mut LspBackend,
    req: Request,
) -> Result<()> {
    match req.method.as_str() {
        "textDocument/completion" => {
            let params: CompletionParams = serde_json::from_value(req.params)
                .map_err(|e| Error::Config(format!("Bad completion params: {e}")))?;
            let result = handlers::handle_completion(backend, &params);
            let response = Response {
                id: req.id,
                result: Some(serde_json::to_value(result).unwrap_or(serde_json::Value::Null)),
                error: None,
            };
            connection
                .sender
                .send(Message::Response(response))
                .map_err(|e| Error::Config(format!("Send failed: {e}")))?;
        }
        "textDocument/hover" => {
            let params: HoverParams = serde_json::from_value(req.params)
                .map_err(|e| Error::Config(format!("Bad hover params: {e}")))?;
            let result = handlers::handle_hover(backend, &params);
            let response = Response {
                id: req.id,
                result: Some(serde_json::to_value(result).unwrap_or(serde_json::Value::Null)),
                error: None,
            };
            connection
                .sender
                .send(Message::Response(response))
                .map_err(|e| Error::Config(format!("Send failed: {e}")))?;
        }
        "textDocument/diagnostic" => {
            let params: DocumentDiagnosticParams = serde_json::from_value(req.params)
                .map_err(|e| Error::Config(format!("Bad diagnostic params: {e}")))?;
            let result = handlers::handle_diagnostic(backend, &params);
            let response = Response {
                id: req.id,
                result: Some(serde_json::to_value(result).unwrap()),
                error: None,
            };
            connection
                .sender
                .send(Message::Response(response))
                .map_err(|e| Error::Config(format!("Send failed: {e}")))?;
        }
        "textDocument/documentSymbol" => {
            let response = Response {
                id: req.id,
                result: Some(serde_json::json!([])),
                error: None,
            };
            connection
                .sender
                .send(Message::Response(response))
                .map_err(|e| Error::Config(format!("Send failed: {e}")))?;
        }
        "textDocument/definition" => {
            let params: GotoDefinitionParams = serde_json::from_value(req.params)
                .map_err(|e| Error::Config(format!("Bad goto def params: {e}")))?;
            let result = handlers::handle_goto_definition(backend, &params);
            let response = Response {
                id: req.id,
                result: Some(serde_json::to_value(result).unwrap_or(serde_json::Value::Null)),
                error: None,
            };
            connection
                .sender
                .send(Message::Response(response))
                .map_err(|e| Error::Config(format!("Send failed: {e}")))?;
        }
        _ => {}
    }
    Ok(())
}

fn handle_notification(_backend: &mut LspBackend, not: Notification) -> Result<()> {
    match not.method.as_str() {
        "textDocument/didOpen" => {
            let params: DidOpenTextDocumentParams = serde_json::from_value(not.params)
                .map_err(|e| Error::Config(format!("Bad didOpen params: {e}")))?;
            _backend.open_document(params.text_document.uri, &params.text_document.text);
        }
        "textDocument/didChange" => {
            let params: DidChangeTextDocumentParams = serde_json::from_value(not.params)
                .map_err(|e| Error::Config(format!("Bad didChange params: {e}")))?;
            if let Some(change) = params.content_changes.into_iter().last() {
                _backend.update_document(params.text_document.uri, &change.text);
            }
        }
        "textDocument/didClose" => {
            let params: DidCloseTextDocumentParams = serde_json::from_value(not.params)
                .map_err(|e| Error::Config(format!("Bad didClose params: {e}")))?;
            _backend.close_document(params.text_document.uri);
        }
        _ => {}
    }
    Ok(())
}
