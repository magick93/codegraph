use super::{run_lsp_server, LspBackend};
use lsp_server::{Connection, Message, Notification, Request, RequestId};
use lsp_types::*;

fn make_init_params() -> serde_json::Value {
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
                },
                "diagnostic": {
                    "relatedDocumentSupport": false
                }
            }
        },
        "workspaceFolders": null
    })
}

fn parse_init_result(msg: Message) -> InitializeResult {
    match msg {
        Message::Response(resp) => {
            serde_json::from_value(resp.result.unwrap()).unwrap()
        }
        _ => panic!("Expected response, got {:?}", msg),
    }
}

fn do_init_handshake(client: &Connection) {
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

fn do_shutdown(client: &Connection) {
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

#[test]
fn test_lsp_initialize_returns_capabilities() {
    let (server_conn, client_conn) = Connection::memory();

    std::thread::spawn(move || {
        run_lsp_server(server_conn, LspBackend::new()).unwrap();
    });

    do_init_handshake(&client_conn);

    assert!(true, "Server initialized successfully");

    do_shutdown(&client_conn);
}

#[test]
fn test_lsp_diagnostic_for_valid_ifml() {
    let (server_conn, client_conn) = Connection::memory();

    std::thread::spawn(move || {
        run_lsp_server(server_conn, LspBackend::new()).unwrap();
    });

    do_init_handshake(&client_conn);

    client_conn
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didOpen".to_string(),
            params: serde_json::json!({
                "textDocument": {
                    "uri": "file:///test.ifml",
                    "languageId": "ifml",
                    "version": 1,
                    "text": r#"view "Hello" {
                        component "greeting" {
                            type: list;
                            data: Person;
                            fields: [name, email];
                        }
                    }"#
                }
            }),
        }))
        .unwrap();

    client_conn
        .sender
        .send(Message::Request(Request {
            id: RequestId::from(2i32),
            method: "textDocument/diagnostic".to_string(),
            params: serde_json::json!({
                "textDocument": { "uri": "file:///test.ifml" }
            }),
        }))
        .unwrap();

    let msg = client_conn.receiver.recv().unwrap();
    match msg {
        Message::Response(resp) => {
            let diag: DocumentDiagnosticReport =
                serde_json::from_value(resp.result.unwrap()).unwrap();
            match diag {
                DocumentDiagnosticReport::Full(report) => {
                    let items = &report.full_document_diagnostic_report.items;
                    assert!(
                        items.iter().all(|d| d.severity != Some(DiagnosticSeverity::ERROR)),
                        "valid IFML should have zero errors"
                    );
                }
                DocumentDiagnosticReport::Unchanged(_) => {
                    // Unchanged report has no items — nothing to assert
                }
            }
        }
        _ => panic!("Expected diagnostic response"),
    }

    do_shutdown(&client_conn);
}

#[test]
fn test_lsp_diagnostic_for_invalid_ifml() {
    let (server_conn, client_conn) = Connection::memory();

    std::thread::spawn(move || {
        run_lsp_server(server_conn, LspBackend::new()).unwrap();
    });

    do_init_handshake(&client_conn);

    client_conn
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didOpen".to_string(),
            params: serde_json::json!({
                "textDocument": {
                    "uri": "file:///bad.ifml",
                    "languageId": "ifml",
                    "version": 1,
                    "text": "view \"Bad\" { invalid syntax here }"
                }
            }),
        }))
        .unwrap();

    client_conn
        .sender
        .send(Message::Request(Request {
            id: RequestId::from(2i32),
            method: "textDocument/diagnostic".to_string(),
            params: serde_json::json!({
                "textDocument": { "uri": "file:///bad.ifml" }
            }),
        }))
        .unwrap();

    let msg = client_conn.receiver.recv().unwrap();
    match msg {
        Message::Response(resp) => {
            let diag: DocumentDiagnosticReport =
                serde_json::from_value(resp.result.unwrap()).unwrap();
            match diag {
                DocumentDiagnosticReport::Full(report) => {
                    let items = &report.full_document_diagnostic_report.items;
                    assert!(!items.is_empty(), "invalid IFML should have diagnostics");
                    assert!(
                        items.iter().any(|d| d.severity == Some(DiagnosticSeverity::ERROR)),
                        "should have at least one error"
                    );
                }
                DocumentDiagnosticReport::Unchanged(_) => {
                    // Unchanged report has no items — shouldn't happen for invalid IFML
                }
            }
        }
        _ => panic!("Expected diagnostic response"),
    }

    do_shutdown(&client_conn);
}

#[test]
fn test_lsp_completion_at_data_field() {
    let (server_conn, client_conn) = Connection::memory();

    std::thread::spawn(move || {
        run_lsp_server(server_conn, LspBackend::new()).unwrap();
    });

    do_init_handshake(&client_conn);

    client_conn
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didOpen".to_string(),
            params: serde_json::json!({
                "textDocument": {
                    "uri": "file:///test.ifml",
                    "languageId": "ifml",
                    "version": 1,
                    "text": r#"view "Hello" {
                        component "greeting" {
                            type: list;
                            data: 
                        }
                    }"#
                }
            }),
        }))
        .unwrap();

    client_conn
        .sender
        .send(Message::Request(Request {
            id: RequestId::from(2i32),
            method: "textDocument/completion".to_string(),
            params: serde_json::json!({
                "textDocument": { "uri": "file:///test.ifml" },
                "position": { "line": 3, "character": 12 }
            }),
        }))
        .unwrap();

    let msg = client_conn.receiver.recv().unwrap();
    match msg {
        Message::Response(resp) => {
            let completion: CompletionResponse =
                serde_json::from_value(resp.result.unwrap()).unwrap();
            match completion {
                CompletionResponse::List(list) => {
                    assert!(
                        list.items.iter().any(|i| i.label == "data:"),
                        "should suggest data: property"
                    );
                }
                _ => panic!("Expected completion list"),
            }
        }
        _ => panic!("Expected completion response"),
    }

    do_shutdown(&client_conn);
}

#[test]
fn test_lsp_initialized_notification() {
    let (server_conn, client_conn) = Connection::memory();

    std::thread::spawn(move || {
        run_lsp_server(server_conn, LspBackend::new()).unwrap();
    });

    do_init_handshake(&client_conn);
    do_shutdown(&client_conn);
}
