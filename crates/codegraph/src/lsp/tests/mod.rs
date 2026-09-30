use auto_lsp::lsp_server::{Connection, Message, Notification, Request, RequestId};
use auto_lsp::lsp_types::*;
use std::sync::Mutex;

use super::{run_lsp_server, GrafeoState};

mod code_action;
mod completion;
mod diagnostics;
mod hover_goto;
mod mox_state;
mod semantic_tokens;
mod update_positions;

pub(super) use crate::lsp::init_mox;

/// Serialize LSP tests that use the shared GRAFE global
pub(super) static LSP_TEST_LOCK: Mutex<()> = Mutex::new(());

pub(super) fn make_init_params() -> serde_json::Value {
    serde_json::json!({
        "processId": null,
        "capabilities": {
            "textDocument": {
                "completion": {
                    "completionItem": {
                        "snippetSupport": false
                    }
                },
                "hover": {
                    "contentFormat": ["markdown"]
                }
            }
        },
        "initializationOptions": {
            "perFileParser": {
                "ifml": "ifml"
            }
        },
        "workspaceFolders": null
    })
}

pub(super) fn parse_init_result(msg: Message) -> InitializeResult {
    match msg {
        Message::Response(resp) => serde_json::from_value(resp.result.unwrap()).unwrap(),
        _ => panic!("Expected response, got {:?}", msg),
    }
}

pub(super) fn do_init_handshake(client: &Connection) {
    client
        .sender
        .send(Message::Request(Request {
            id: RequestId::from(1i32),
            method: "initialize".to_string(),
            params: make_init_params(),
        }))
        .unwrap();
    let msg = client.receiver.recv().unwrap();
    let _result = parse_init_result(msg);
    client
        .sender
        .send(Message::Notification(Notification {
            method: "initialized".to_string(),
            params: serde_json::json!({}),
        }))
        .unwrap();
}

pub(super) fn do_shutdown(client: &Connection) {
    client
        .sender
        .send(Message::Request(Request {
            id: RequestId::from(99i32),
            method: "shutdown".to_string(),
            params: serde_json::json!(null),
        }))
        .unwrap();
    let msg = client.receiver.recv().unwrap();
    match msg {
        Message::Response(resp) => {
            assert_eq!(resp.result, Some(serde_json::json!(null)));
        }
        _ => panic!("Expected shutdown response"),
    }
    client
        .sender
        .send(Message::Notification(Notification {
            method: "exit".to_string(),
            params: serde_json::json!(null),
        }))
        .unwrap();
}

pub(super) fn open_document(client: &Connection, uri: &str, text: &str) {
    client
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didOpen".to_string(),
            params: serde_json::json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": "ifml",
                    "version": 1,
                    "text": text
                }
            }),
        }))
        .unwrap();
}

pub(super) fn recv_diagnostics(
    client: &Connection,
    expected_uri: &str,
) -> PublishDiagnosticsParams {
    loop {
        let msg = client.receiver.recv().unwrap();
        match msg {
            Message::Notification(not) if not.method == "textDocument/publishDiagnostics" => {
                let params: PublishDiagnosticsParams = serde_json::from_value(not.params).unwrap();
                assert_eq!(params.uri.as_str(), expected_uri);
                return params;
            }
            Message::Notification(_) => {
                // skip other notifications (e.g. window/showMessage)
                continue;
            }
            other => panic!("Expected publishDiagnostics notification, got {:?}", other),
        }
    }
}

#[test]
fn test_lsp_initialize_returns_capabilities() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
    let (server_conn, client_conn) = Connection::memory();

    std::thread::spawn(move || {
        run_lsp_server(server_conn, GrafeoState::default()).unwrap();
    });

    do_init_handshake(&client_conn);

    do_shutdown(&client_conn);
}
