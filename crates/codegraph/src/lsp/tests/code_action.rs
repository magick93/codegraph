use std::collections::HashMap;

use auto_lsp::lsp_server::{Connection, Message, Request, RequestId};
use auto_lsp::lsp_types::*;

use super::{LSP_TEST_LOCK, do_init_handshake, do_shutdown, open_document, recv_diagnostics};
use crate::lsp::{GrafeoState, run_lsp_server};

#[test]
fn test_lsp_code_action_missing_entity() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
    let (server_conn, client_conn) = Connection::memory();

    let state = GrafeoState {
        entity_names: vec!["Customer".to_string()],
        schema_infos: HashMap::new(),
        schema_dirs: vec![],
    };

    std::thread::spawn(move || {
        run_lsp_server(server_conn, state).unwrap();
    });

    do_init_handshake(&client_conn);

    open_document(
        &client_conn,
        "file:///test.ifml",
        r#"view "Test" { component "c" { type: list; data: Order; } }"#,
    );

    let _ = recv_diagnostics(&client_conn, "file:///test.ifml");

    client_conn
        .sender
        .send(Message::Request(Request {
            id: RequestId::from(10i32),
            method: "textDocument/codeAction".to_string(),
            params: serde_json::json!({
                "textDocument": { "uri": "file:///test.ifml" },
                "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 50 } },
                "context": {
                    "diagnostics": [],
                    "triggerKind": 2
                }
            }),
        }))
        .unwrap();

    let msg = client_conn.receiver.recv().unwrap();
    match msg {
        Message::Response(resp) => {
            let result: Option<Vec<CodeActionOrCommand>> =
                serde_json::from_value(resp.result.unwrap_or(serde_json::Value::Null)).ok();
            match result {
                Some(actions) => {
                    assert!(!actions.is_empty(), "Should have at least one code action");
                    let titles: Vec<String> = actions
                        .iter()
                        .map(|a| match a {
                            CodeActionOrCommand::CodeAction(ca) => ca.title.clone(),
                            CodeActionOrCommand::Command(cmd) => cmd.title.clone(),
                        })
                        .collect();
                    assert!(
                        titles.iter().any(|t| t.contains("Create schema")),
                        "Should have 'Create schema' action, got: {:?}",
                        titles
                    );
                }
                None => panic!("Expected code action response"),
            }
        }
        _ => panic!("Expected response"),
    }

    do_shutdown(&client_conn);
}
