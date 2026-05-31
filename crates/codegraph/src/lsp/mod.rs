use ast_ifml::db::IFML_PARSERS;
use auto_lsp::default::db::{BaseDb, FileManager};
use auto_lsp::default::server::file_events::open_text_document;
use auto_lsp::default::server::workspace_init::WorkspaceInit;
use auto_lsp::lsp_server::Connection;
use auto_lsp::lsp_types::*;
use auto_lsp::lsp_types::{
    notification::{
        Cancel, DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument,
        Initialized, PublishDiagnostics, SetTrace,
    },
    request::{
        CodeActionRequest, Completion, DocumentDiagnosticRequest, GotoDefinition, HoverRequest,
        SemanticTokensFullRequest,
    },
};
use auto_lsp::server::notification_registry::NotificationRegistry;
use auto_lsp::server::options::InitOptions;
use auto_lsp::server::request_registry::RequestRegistry;
use auto_lsp::server::Session;

pub use state::*;

mod handlers;
mod state;
#[cfg(test)]
mod tests;

pub fn run_lsp_server(
    connection: Connection,
    grafeo_state: GrafeoState,
) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    init_grafe(grafeo_state);

    let db = BaseDb::default();
    let init_options = InitOptions {
        parsers: &*IFML_PARSERS,
        capabilities: server_capabilities(),
        server_info: None,
    };

    let (mut session, init_params) = Session::create(init_options, connection, db)
        .map_err(|e| {
            // Provide a helpful message if the client forgot initializationOptions
            let msg = e.to_string();
            if msg.contains("MissingPerFileParser") || msg.contains("perFileParser") {
                eprintln!(
                    "ERROR: LSP client did not send initializationOptions.perFileParser.\n\
                     The VS Code extension sends this automatically. If using a custom client,\n\
                     include: {{ \"initializationOptions\": {{ \"perFileParser\": {{ \"ifml\": \"ifml\" }} }} }}"
                );
            }
            e
        })?;

    let mut request_registry = RequestRegistry::<BaseDb>::default();
    let mut notification_registry = NotificationRegistry::<BaseDb>::default();

    request_registry
        .on::<Completion, _>(handlers::handle_completion)
        .on::<HoverRequest, _>(handlers::handle_hover)
        .on::<GotoDefinition, _>(handlers::handle_goto_definition)
        .on::<DocumentDiagnosticRequest, _>(handlers::handle_document_diagnostic)
        .on::<CodeActionRequest, _>(handlers::handle_code_action)
        .on::<SemanticTokensFullRequest, _>(handlers::handle_semantic_tokens_full);

    notification_registry
        .on::<Initialized, _>(|_db, _params| Ok(()))
        .on::<SetTrace, _>(|_db, _params| Ok(()))
        .on::<Cancel, _>(|_db, _params| Ok(()))
        .on::<DidSaveTextDocument, _>(|_db, _params| Ok(()))
        .on_mut::<DidOpenTextDocument, _>(|session, params| {
            let uri = params.text_document.uri.clone();
            open_text_document(session, params)?;
            push_diagnostics(session, &uri);
            Ok(())
        })
        .on_mut::<DidChangeTextDocument, _>(|session, params| {
            let uri = params.text_document.uri.clone();
            let _ = session.db.update(&uri, &params.content_changes);
            push_diagnostics(session, &uri);
            Ok(())
        })
        .on_mut::<DidCloseTextDocument, _>(|session, params| {
            let _ = session.db.remove_file(&params.text_document.uri);
            let params = PublishDiagnosticsParams {
                uri: params.text_document.uri,
                diagnostics: vec![],
                version: None,
            };
            let _ = session.send_notification::<PublishDiagnostics>(params);
            Ok(())
        });

    session.init_workspace(init_params)?;
    session.main_loop(&request_registry, &notification_registry)?;

    Ok(())
}

fn push_diagnostics(session: &mut Session<BaseDb>, uri: &Url) {
    let diagnostics = handlers::compute_diagnostics(&session.db, uri);
    let params = PublishDiagnosticsParams {
        uri: uri.clone(),
        diagnostics,
        version: None,
    };
    if let Err(e) = session.send_notification::<PublishDiagnostics>(params) {
        eprintln!("Failed to send diagnostics: {e}");
    }
}

fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec![":".to_string(), "\"".to_string(), ".".to_string()]),
            ..Default::default()
        }),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),
        semantic_tokens_provider: Some(
            SemanticTokensServerCapabilities::SemanticTokensOptions(SemanticTokensOptions {
                legend: SemanticTokensLegend {
                    token_types: handlers::TOKEN_TYPES
                        .iter()
                        .map(|s| SemanticTokenType::new(s))
                        .collect(),
                    token_modifiers: handlers::TOKEN_MODIFIERS
                        .iter()
                        .map(|s| SemanticTokenModifier::new(s))
                        .collect(),
                },
                full: Some(SemanticTokensFullOptions::Bool(true)),
                range: None,
                ..Default::default()
            }),
        ),
        diagnostic_provider: Some(DiagnosticServerCapabilities::Options(DiagnosticOptions {
            identifier: None,
            inter_file_dependencies: true,
            workspace_diagnostics: false,
            work_done_progress_options: WorkDoneProgressOptions { work_done_progress: None },
        })),
        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
        ..Default::default()
    }
}
