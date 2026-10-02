use auto_lsp::lsp_server::{Connection, Message, Request, RequestId};
use auto_lsp::lsp_types::*;

use super::{LSP_TEST_LOCK, do_init_handshake, do_shutdown, open_document, recv_diagnostics};
use crate::lsp::{GrafeoState, run_lsp_server};

#[test]
fn test_lsp_semantic_tokens() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
    let (server_conn, client_conn) = Connection::memory();

    std::thread::spawn(move || {
        run_lsp_server(server_conn, GrafeoState::default()).unwrap();
    });

    do_init_handshake(&client_conn);

    open_document(
        &client_conn,
        "file:///test.ifml",
        r#"view "Hello" { component "c" { type: list; data: Customer; } }"#,
    );

    let _ = recv_diagnostics(&client_conn, "file:///test.ifml");

    client_conn
        .sender
        .send(Message::Request(Request {
            id: RequestId::from(10i32),
            method: "textDocument/semanticTokens/full".to_string(),
            params: serde_json::json!({
                "textDocument": { "uri": "file:///test.ifml" }
            }),
        }))
        .unwrap();

    let msg = client_conn.receiver.recv().unwrap();
    match msg {
        Message::Response(resp) => {
            let result: Option<SemanticTokensResult> =
                serde_json::from_value(resp.result.unwrap_or(serde_json::Value::Null))
                    .ok()
                    .flatten();
            match result {
                Some(SemanticTokensResult::Tokens(tokens)) => {
                    assert!(!tokens.data.is_empty(), "Should produce semantic tokens");
                }
                _ => panic!("Expected SemanticTokensResult::Tokens"),
            }
        }
        _ => panic!("Expected response"),
    }

    do_shutdown(&client_conn);
}
