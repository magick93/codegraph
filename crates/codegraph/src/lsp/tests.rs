use auto_lsp::lsp_server::{Connection, Message, Notification, Request, RequestId};
use auto_lsp::lsp_types::*;

use super::{run_lsp_server, GrafeoState, SchemaInfo};

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

fn open_document(client: &Connection, uri: &str, text: &str) {
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

fn recv_diagnostics(
    client: &Connection,
    expected_uri: &str,
) -> PublishDiagnosticsParams {
    loop {
        let msg = client.receiver.recv().unwrap();
        match msg {
            Message::Notification(not) if not.method == "textDocument/publishDiagnostics" => {
                let params: PublishDiagnosticsParams =
                    serde_json::from_value(not.params).unwrap();
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
    let (server_conn, client_conn) = Connection::memory();

    std::thread::spawn(move || {
        run_lsp_server(server_conn, GrafeoState::default()).unwrap();
    });

    do_init_handshake(&client_conn);
    assert!(true, "Server initialized successfully");

    do_shutdown(&client_conn);
}

#[test]
fn test_lsp_diagnostic_for_valid_ifml() {
    let (server_conn, client_conn) = Connection::memory();

    std::thread::spawn(move || {
        run_lsp_server(server_conn, GrafeoState::default()).unwrap();
    });

    do_init_handshake(&client_conn);

    open_document(
        &client_conn,
        "file:///test.ifml",
        r#"view "Hello" {
            component "greeting" {
                type: list;
                data: Person;
                fields: [name, email];
            }
        }"#,
    );

    let params = recv_diagnostics(&client_conn, "file:///test.ifml");
    assert!(
        params.diagnostics.is_empty(),
        "valid IFML should have zero diagnostics, got: {:?}",
        params.diagnostics
    );

    do_shutdown(&client_conn);
}

#[test]
fn test_lsp_diagnostic_for_invalid_ifml() {
    let (server_conn, client_conn) = Connection::memory();

    std::thread::spawn(move || {
        run_lsp_server(server_conn, GrafeoState::default()).unwrap();
    });

    do_init_handshake(&client_conn);

    open_document(
        &client_conn,
        "file:///bad.ifml",
        r#"view "Bad" { invalid syntax here }"#,
    );

    let params = recv_diagnostics(&client_conn, "file:///bad.ifml");
    assert!(
        !params.diagnostics.is_empty(),
        "invalid IFML should have diagnostics"
    );
    assert!(
        params
            .diagnostics
            .iter()
            .any(|d| d.severity == Some(DiagnosticSeverity::ERROR)),
        "should have at least one error"
    );

    do_shutdown(&client_conn);
}

#[test]
fn test_lsp_completion_with_entity_data() {
    let (server_conn, client_conn) = Connection::memory();

    let mut schema_infos = std::collections::HashMap::new();
    schema_infos.insert(
        "Customer".to_string(),
        SchemaInfo {
            title: "Customer".to_string(),
            description: Some("A customer entity".to_string()),
            properties: vec!["name".to_string(), "email".to_string()],
            rel_path: "customer.json".to_string(),
        },
    );
    let state = GrafeoState {
        entity_names: vec!["Customer".to_string()],
        schema_infos,
        schema_dirs: vec![],
    };

    std::thread::spawn(move || {
        run_lsp_server(server_conn, state).unwrap();
    });

    do_init_handshake(&client_conn);

    open_document(
        &client_conn,
        "file:///test.ifml",
        "view \"Hello\" { component \"g\" { data: Customer } }",
    );

    // Wait for diagnostics
    let _ = recv_diagnostics(&client_conn, "file:///test.ifml");

    // Request completion after "data: " — position at index 36 is right after "data: "
    // "view \"Hello\" { component \"g\" { data: Customer } }"
    //                                               ^-- 36
    client_conn
        .sender
        .send(Message::Request(Request {
            id: RequestId::from(2i32),
            method: "textDocument/completion".to_string(),
            params: serde_json::json!({
                "textDocument": { "uri": "file:///test.ifml" },
                "position": { "line": 0, "character": 36 }
            }),
        }))
        .unwrap();

    let msg = client_conn.receiver.recv().unwrap();
    match msg {
        Message::Response(resp) => {
            let result = resp.result.unwrap_or(serde_json::Value::Null);
            assert!(!result.is_null(), "completion should return results");
            let completion: CompletionResponse =
                serde_json::from_value(result).unwrap();
            match completion {
                CompletionResponse::List(list) => {
                    assert!(
                        list.items.iter().any(|i| i.label == "Customer"),
                        "should suggest Customer entity, got labels: {:?}",
                        list.items.iter().map(|i| &i.label).collect::<Vec<_>>()
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
fn test_lsp_goto_definition_view() {
    let (server_conn, client_conn) = Connection::memory();

    std::thread::spawn(move || {
        run_lsp_server(server_conn, GrafeoState::default()).unwrap();
    });

    do_init_handshake(&client_conn);

    open_document(
        &client_conn,
        "file:///test.ifml",
        r#"view "CustomerList" { component "c" { type: list; data: Customer; } }
view "CustomerDetail" { component "d" { type: details; data: Customer; } }"#,
    );

    let _ = recv_diagnostics(&client_conn, "file:///test.ifml");

    client_conn
        .sender
        .send(Message::Request(Request {
            id: RequestId::from(2i32),
            method: "textDocument/definition".to_string(),
            params: serde_json::json!({
                "textDocument": { "uri": "file:///test.ifml" },
                "position": { "line": 0, "character": 8 }
            }),
        }))
        .unwrap();

    let msg = client_conn.receiver.recv().unwrap();
    match msg {
        Message::Response(resp) => {
            assert!(resp.result.is_some(), "Should find view declaration");
            if let Some(result) = resp.result {
                let def: GotoDefinitionResponse = serde_json::from_value(result).unwrap();
                match def {
                    GotoDefinitionResponse::Scalar(loc) => {
                        assert_eq!(loc.uri.as_str(), "file:///test.ifml");
                        assert_eq!(loc.range.start.line, 0);
                    }
                    _ => panic!("Expected Scalar definition"),
                }
            }
        }
        _ => panic!("Expected response"),
    }

    do_shutdown(&client_conn);
}

#[test]
fn test_lsp_goto_definition_entity_no_file() {
    let (server_conn, client_conn) = Connection::memory();

    let mut schema_infos = std::collections::HashMap::new();
    schema_infos.insert(
        "Customer".to_string(),
        SchemaInfo {
            title: "Customer".to_string(),
            description: None,
            properties: vec!["name".to_string(), "email".to_string()],
            rel_path: "customer.json".to_string(),
        },
    );
    let state = GrafeoState {
        entity_names: vec!["Customer".to_string()],
        schema_infos,
        schema_dirs: vec![],
    };

    std::thread::spawn(move || {
        run_lsp_server(server_conn, state).unwrap();
    });

    do_init_handshake(&client_conn);

    open_document(
        &client_conn,
        "file:///test.ifml",
        r#"view "Test" { component "c" { type: list; data: Customer; } }"#,
    );

    let _ = recv_diagnostics(&client_conn, "file:///test.ifml");

    client_conn
        .sender
        .send(Message::Request(Request {
            id: RequestId::from(2i32),
            method: "textDocument/definition".to_string(),
            params: serde_json::json!({
                "textDocument": { "uri": "file:///test.ifml" },
                "position": { "line": 0, "character": 60 }
            }),
        }))
        .unwrap();

    let msg = client_conn.receiver.recv().unwrap();
    match msg {
        Message::Response(resp) => {
            // Result may be None or Null if schema file doesn't exist on disk
            match resp.result {
                None => {} // result field absent
                Some(val) if val.is_null() => {} // result is null
                other => panic!("Expected null result (no schema file), got: {:?}", other),
            }
        }
        _ => panic!("Expected response"),
    }

    do_shutdown(&client_conn);
}
