use auto_lsp::lsp_server::{Connection, Message, Notification, Request, RequestId};
use auto_lsp::lsp_types::*;
use std::collections::HashMap;
use std::sync::Mutex;

use super::{run_lsp_server, GrafeoState, SchemaInfo};

/// Serialize LSP tests that use the shared GRAFE global
static LSP_TEST_LOCK: Mutex<()> = Mutex::new(());

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
        Message::Response(resp) => serde_json::from_value(resp.result.unwrap()).unwrap(),
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

fn recv_diagnostics(client: &Connection, expected_uri: &str) -> PublishDiagnosticsParams {
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

#[test]
fn test_lsp_diagnostic_for_valid_ifml() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
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
    let _lock = LSP_TEST_LOCK.lock().unwrap();
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

/// Regression test: entity names have the "Type" suffix stripped
/// (CustomerType → Customer). Verifies that data: Customer passes
/// when entity_names contains "Customer", while data: Nonexistent fails.
#[test]
fn test_lsp_diagnostic_entity_suffix_stripped() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
    let (server_conn, client_conn) = Connection::memory();

    // Simulate what the server builds: schema is CustomerType but entity_names
    // has the stripped name "Customer" after AutoClassifier + suffix strip
    let mut schema_infos = std::collections::HashMap::new();
    schema_infos.insert(
        "Customer".to_string(),
        SchemaInfo {
            title: "CustomerType".to_string(),
            description: Some("A customer".to_string()),
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

    // data: Customer — Customer IS in entity_names (stripped from CustomerType).
    // Should NOT produce any entity-not-found error.
    open_document(
        &client_conn,
        "file:///valid.ifml",
        r#"view "Test" { component "c" { type: list; data: Customer; fields: [name, email]; } }"#,
    );

    let params = recv_diagnostics(&client_conn, "file:///valid.ifml");
    assert!(
        !params
            .diagnostics
            .iter()
            .any(|d| d.message.contains("Entity")),
        "data: Customer should NOT produce entity error. Got: {:?}",
        params
            .diagnostics
            .iter()
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );

    // Now open a second document with data: Nonexistent — should error
    open_document(
        &client_conn,
        "file:///invalid.ifml",
        r#"view "Test" { component "c" { type: list; data: Nonexistent; } }"#,
    );

    let params = recv_diagnostics(&client_conn, "file:///invalid.ifml");
    assert!(
        params
            .diagnostics
            .iter()
            .any(|d| d.message.contains("Entity") && d.message.contains("Nonexistent")),
        "data: Nonexistent SHOULD produce entity error"
    );

    do_shutdown(&client_conn);
}

#[test]
fn test_lsp_diagnostic_for_missing_entity() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
    let (server_conn, client_conn) = Connection::memory();

    let state = GrafeoState {
        entity_names: vec!["Customer".to_string()],
        schema_infos: std::collections::HashMap::new(),
        schema_dirs: vec![],
    };

    std::thread::spawn(move || {
        run_lsp_server(server_conn, state).unwrap();
    });

    do_init_handshake(&client_conn);

    // data: Order — Order is NOT in entity_names (only Customer is)
    open_document(
        &client_conn,
        "file:///bad_entity.ifml",
        r#"view "Test" { component "c" { type: list; data: Order; fields: [name]; } }"#,
    );

    let params = recv_diagnostics(&client_conn, "file:///bad_entity.ifml");
    assert!(
        !params.diagnostics.is_empty(),
        "referencing unknown entity 'Order' should produce diagnostics"
    );
    assert!(
        params
            .diagnostics
            .iter()
            .any(|d| { d.message.contains("Entity") && d.message.contains("Order") }),
        "should have error about unknown entity 'Order', got: {:?}",
        params
            .diagnostics
            .iter()
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );

    do_shutdown(&client_conn);
}

#[test]
fn test_lsp_diagnostic_invalid_param_binding() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
    let (server_conn, client_conn) = Connection::memory();

    std::thread::spawn(move || {
        run_lsp_server(server_conn, GrafeoState::default()).unwrap();
    });

    do_init_handshake(&client_conn);

    // Detail has params { customerId: Uuid }
    // List navigates to Detail with wrongKey — invalid
    open_document(
        &client_conn,
        "file:///test.ifml",
        r#"view "List" {
            component "c" {
                type: list;
                data: Customer;
                on select -> navigate("Detail", { wrongKey: row.id });
            }
        }

view "Detail" {
    params { customerId: Uuid };
    component "d" {
        type: details;
        data: Customer;
    }
}"#,
    );

    let params = recv_diagnostics(&client_conn, "file:///test.ifml");
    assert!(
        params.diagnostics.iter().any(|d| {
            d.message.contains("wrongKey") && d.message.contains("not a declared parameter")
        }),
        "Should warn about invalid parameter binding 'wrongKey', got: {:?}",
        params
            .diagnostics
            .iter()
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );

    do_shutdown(&client_conn);
}

#[test]
fn test_lsp_no_false_duplicate_across_views() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
    let (server_conn, client_conn) = Connection::memory();
    std::thread::spawn(move || {
        run_lsp_server(server_conn, GrafeoState::default()).unwrap();
    });
    do_init_handshake(&client_conn);

    // Two views with overlapping field names should NOT flag as duplicates
    open_document(
        &client_conn,
        "file:///test.ifml",
        r#"view "A" { component "c1" { type: list; data: Customer; fields: [name, email]; } }
view "B" { component "c2" { type: list; data: Order; fields: [name, email]; } }"#,
    );

    let params = recv_diagnostics(&client_conn, "file:///test.ifml");
    let dups: Vec<&Diagnostic> = params
        .diagnostics
        .iter()
        .filter(|d| d.message.contains("Duplicate"))
        .collect();
    assert!(
        dups.is_empty(),
        "Fields in different views should not be flagged as duplicates. Got: {:?}",
        dups.iter().map(|d| &d.message).collect::<Vec<_>>()
    );

    do_shutdown(&client_conn);
}

#[test]
fn test_lsp_duplicate_field_in_same_array() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
    let (server_conn, client_conn) = Connection::memory();
    std::thread::spawn(move || {
        run_lsp_server(server_conn, GrafeoState::default()).unwrap();
    });
    do_init_handshake(&client_conn);

    // Duplicate 'email' in the SAME fields array should flag as duplicate
    open_document(
        &client_conn,
        "file:///test.ifml",
        r#"view "A" { component "c" { type: list; data: Customer; fields: [name, email, email]; } }"#,
    );

    let params = recv_diagnostics(&client_conn, "file:///test.ifml");
    let dups: Vec<&Diagnostic> = params
        .diagnostics
        .iter()
        .filter(|d| d.message.contains("Duplicate"))
        .collect();
    assert!(
        !dups.is_empty(),
        "Duplicate 'email' in same array should be flagged"
    );

    do_shutdown(&client_conn);
}

#[test]
fn test_lsp_completion_with_entity_data() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
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
            let completion: CompletionResponse = serde_json::from_value(result).unwrap();
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
fn test_lsp_completion_component_body_statements() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
    let (server_conn, client_conn) = Connection::memory();

    std::thread::spawn(move || {
        run_lsp_server(server_conn, GrafeoState::default()).unwrap();
    });

    do_init_handshake(&client_conn);

    open_document(
        &client_conn,
        "file:///test.ifml",
        "view \"Hello\" {\n    component \"g\" {\n        type: list;\n\n    }\n}",
    );

    let _ = recv_diagnostics(&client_conn, "file:///test.ifml");

    // Completion on the empty line inside the component body (line 3)
    client_conn
        .sender
        .send(Message::Request(Request {
            id: RequestId::from(2i32),
            method: "textDocument/completion".to_string(),
            params: serde_json::json!({
                "textDocument": { "uri": "file:///test.ifml" },
                "position": { "line": 3, "character": 0 }
            }),
        }))
        .unwrap();

    let msg = client_conn.receiver.recv().unwrap();
    match msg {
        Message::Response(resp) => {
            let result = resp.result.unwrap_or(serde_json::Value::Null);
            assert!(!result.is_null(), "completion should return results");
            let completion: CompletionResponse = serde_json::from_value(result).unwrap();
            match completion {
                CompletionResponse::List(list) => {
                    let labels: Vec<&str> = list.items.iter().map(|i| i.label.as_str()).collect();
                    for expected in [
                        "column",
                        "column lookup",
                        "column expr",
                        "field",
                        "chart",
                        "chart body",
                    ] {
                        assert!(
                            labels.contains(&expected),
                            "should suggest '{expected}', got labels: {labels:?}"
                        );
                    }
                    assert!(
                        labels.contains(&"type:"),
                        "property completions should still be offered"
                    );
                    let column = list
                        .items
                        .iter()
                        .find(|i| i.label == "column")
                        .expect("column item present");
                    assert!(
                        column
                            .insert_text
                            .as_deref()
                            .is_some_and(|t| t.contains("-> field")),
                        "column snippet should insert a typed column, got {column:?}"
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
fn test_lsp_completion_no_statements_outside_component() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
    let (server_conn, client_conn) = Connection::memory();

    std::thread::spawn(move || {
        run_lsp_server(server_conn, GrafeoState::default()).unwrap();
    });

    do_init_handshake(&client_conn);

    open_document(
        &client_conn,
        "file:///test.ifml",
        "view \"Hello\" {\n    component \"g\" {\n        type: list;\n    }\n\n}",
    );

    let _ = recv_diagnostics(&client_conn, "file:///test.ifml");

    // Empty line inside the view body (line 4), NOT inside a component
    client_conn
        .sender
        .send(Message::Request(Request {
            id: RequestId::from(2i32),
            method: "textDocument/completion".to_string(),
            params: serde_json::json!({
                "textDocument": { "uri": "file:///test.ifml" },
                "position": { "line": 4, "character": 0 }
            }),
        }))
        .unwrap();

    let msg = client_conn.receiver.recv().unwrap();
    match msg {
        Message::Response(resp) => {
            let result = resp.result.unwrap_or(serde_json::Value::Null);
            if result.is_null() {
                // no completions at all is acceptable outside components
            } else {
                let completion: CompletionResponse = serde_json::from_value(result).unwrap();
                if let CompletionResponse::List(list) = completion {
                    assert!(
                        list.items.iter().all(|i| {
                            !i.label.starts_with("column") && !i.label.starts_with("chart")
                        }),
                        "statement snippets must not appear in view body, got: {:?}",
                        list.items.iter().map(|i| &i.label).collect::<Vec<_>>()
                    );
                }
            }
        }
        _ => panic!("Expected completion response"),
    }

    do_shutdown(&client_conn);
}

#[test]
fn test_lsp_goto_definition_view() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
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

#[test]
fn test_lsp_goto_definition_entity_no_file() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
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
                None => {}                       // result field absent
                Some(val) if val.is_null() => {} // result is null
                other => panic!("Expected null result (no schema file), got: {:?}", other),
            }
        }
        _ => panic!("Expected response"),
    }

    do_shutdown(&client_conn);
}

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

#[test]
fn test_lsp_diagnostic_unknown_field_in_fields() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
    let (server_conn, client_conn) = Connection::memory();

    let mut schema_infos = std::collections::HashMap::new();
    schema_infos.insert(
        "Customer".to_string(),
        SchemaInfo {
            title: "CustomerType".to_string(),
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

    // 'bogus' is not a property of CustomerType — should warn naming the schema title
    open_document(
        &client_conn,
        "file:///bad_field.ifml",
        r#"view "Test" { component "c" { type: list; data: Customer; fields: [name, bogus]; } }"#,
    );

    let params = recv_diagnostics(&client_conn, "file:///bad_field.ifml");
    assert!(
        params.diagnostics.iter().any(|d| {
            d.message.contains("bogus")
                && d.message.contains("Customer")
                && d.message.contains("CustomerType")
        }),
        "should warn about unknown field 'bogus' on entity 'Customer' naming schema title, got: {:?}",
        params
            .diagnostics
            .iter()
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );

    do_shutdown(&client_conn);
}

#[test]
fn test_lsp_no_field_diagnostic_for_known_fields() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
    let (server_conn, client_conn) = Connection::memory();

    let mut schema_infos = std::collections::HashMap::new();
    schema_infos.insert(
        "Customer".to_string(),
        SchemaInfo {
            title: "CustomerType".to_string(),
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
        "file:///good_fields.ifml",
        r#"view "Test" { component "c" { type: list; data: Customer; fields: [name, email]; } }"#,
    );

    let params = recv_diagnostics(&client_conn, "file:///good_fields.ifml");
    assert!(
        !params
            .diagnostics
            .iter()
            .any(|d| d.message.contains("not found on entity")),
        "known fields should not produce field diagnostics, got: {:?}",
        params
            .diagnostics
            .iter()
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );

    do_shutdown(&client_conn);
}

#[test]
fn test_lsp_completion_field_names_in_fields_context() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
    let (server_conn, client_conn) = Connection::memory();

    let mut schema_infos = std::collections::HashMap::new();
    schema_infos.insert(
        "Customer".to_string(),
        SchemaInfo {
            title: "CustomerType".to_string(),
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

    let text = r#"view "A" { component "c" { type: list; data: Customer; fields: [name, ] } }"#;
    open_document(&client_conn, "file:///fields_ctx.ifml", text);

    let _ = recv_diagnostics(&client_conn, "file:///fields_ctx.ifml");

    // Place the cursor right after "fields: [" — the fields context should
    // offer the bound entity's properties.
    let pos = text.find("fields: [").unwrap() + "fields: [".len();
    client_conn
        .sender
        .send(Message::Request(Request {
            id: RequestId::from(2i32),
            method: "textDocument/completion".to_string(),
            params: serde_json::json!({
                "textDocument": { "uri": "file:///fields_ctx.ifml" },
                "position": { "line": 0, "character": pos }
            }),
        }))
        .unwrap();

    let msg = client_conn.receiver.recv().unwrap();
    match msg {
        Message::Response(resp) => {
            let result = resp.result.unwrap_or(serde_json::Value::Null);
            assert!(!result.is_null(), "completion should return results");
            let completion: CompletionResponse = serde_json::from_value(result).unwrap();
            match completion {
                CompletionResponse::List(list) => {
                    let labels: Vec<&str> = list.items.iter().map(|i| i.label.as_str()).collect();
                    assert!(
                        labels.contains(&"email") && labels.contains(&"name"),
                        "fields context should suggest properties of Customer, got: {labels:?}"
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
fn test_lsp_diagnostic_unknown_navigate_target() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
    let (server_conn, client_conn) = Connection::memory();
    std::thread::spawn(move || {
        run_lsp_server(server_conn, GrafeoState::default()).unwrap();
    });
    do_init_handshake(&client_conn);

    // navigate("Missing") — no view "Missing" declared → WARNING
    open_document(
        &client_conn,
        "file:///nav_bad.ifml",
        r#"view "List" {
    component "c" {
        type: list;
        on select -> navigate("Missing", { id: row.id });
    }
}"#,
    );

    let params = recv_diagnostics(&client_conn, "file:///nav_bad.ifml");
    let unknown_view: Vec<&Diagnostic> = params
        .diagnostics
        .iter()
        .filter(|d| d.message.contains("Unknown view"))
        .collect();
    assert!(
        unknown_view.iter().any(|d| d.message.contains("Missing")),
        "should warn about unknown navigate target 'Missing', got: {:?}",
        params
            .diagnostics
            .iter()
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
    assert!(
        unknown_view
            .iter()
            .all(|d| d.severity == Some(DiagnosticSeverity::WARNING)),
        "unknown navigate target should be a warning"
    );

    // navigate("Detail") with view "Detail" declared → no unknown-view warning
    open_document(
        &client_conn,
        "file:///nav_good.ifml",
        r#"view "List" {
    component "c" {
        type: list;
        on select -> navigate("Detail", { id: row.id });
    }
}

view "Detail" { }"#,
    );

    let params = recv_diagnostics(&client_conn, "file:///nav_good.ifml");
    assert!(
        !params
            .diagnostics
            .iter()
            .any(|d| d.message.contains("Unknown view")),
        "valid navigate target should NOT warn, got: {:?}",
        params
            .diagnostics
            .iter()
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );

    do_shutdown(&client_conn);
}

#[test]
fn test_lsp_completion_module_names_after_use() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
    let (server_conn, client_conn) = Connection::memory();

    std::thread::spawn(move || {
        run_lsp_server(server_conn, GrafeoState::default()).unwrap();
    });

    do_init_handshake(&client_conn);

    let text = "module \"Maps\" { input { } output { } }\nview \"A\" {\n    use \"\n}";
    open_document(&client_conn, "file:///use_ctx.ifml", text);

    let _ = recv_diagnostics(&client_conn, "file:///use_ctx.ifml");

    // Cursor right after the opening quote of `use "`
    client_conn
        .sender
        .send(Message::Request(Request {
            id: RequestId::from(2i32),
            method: "textDocument/completion".to_string(),
            params: serde_json::json!({
                "textDocument": { "uri": "file:///use_ctx.ifml" },
                "position": { "line": 2, "character": 9 }
            }),
        }))
        .unwrap();

    let msg = client_conn.receiver.recv().unwrap();
    match msg {
        Message::Response(resp) => {
            let result = resp.result.unwrap_or(serde_json::Value::Null);
            assert!(!result.is_null(), "completion should return results");
            let completion: CompletionResponse = serde_json::from_value(result).unwrap();
            match completion {
                CompletionResponse::List(list) => {
                    let labels: Vec<&str> = list.items.iter().map(|i| i.label.as_str()).collect();
                    assert!(
                        labels.contains(&"Maps"),
                        "completion after `use \"` should suggest declared modules, got: {labels:?}"
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
fn test_lsp_diagnostic_unknown_module_use() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
    let (server_conn, client_conn) = Connection::memory();

    std::thread::spawn(move || {
        run_lsp_server(server_conn, GrafeoState::default()).unwrap();
    });

    do_init_handshake(&client_conn);

    // use "Missing" with no module "Missing" declared → WARNING
    open_document(
        &client_conn,
        "file:///use_bad.ifml",
        "module \"AuditTrail\" { input { } output { } }\nview \"A\" {\n    use \"Missing\";\n}",
    );

    let params = recv_diagnostics(&client_conn, "file:///use_bad.ifml");
    assert!(
        !params.diagnostics.is_empty(),
        "use of undeclared module should produce diagnostics, got none"
    );
    assert!(
        params
            .diagnostics
            .iter()
            .any(|d| d.message.contains("Unknown module")
                && d.message.contains("Missing")
                && d.severity == Some(DiagnosticSeverity::WARNING)),
        "should warn about unknown module 'Missing', got: {:?}",
        params
            .diagnostics
            .iter()
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );

    // use "AuditTrail" with module declared → no unknown-module warning
    open_document(
        &client_conn,
        "file:///use_good.ifml",
        "module \"AuditTrail\" { input { } output { } }\nview \"A\" {\n    use \"AuditTrail\" as audit { scope: org; };\n}",
    );

    let params = recv_diagnostics(&client_conn, "file:///use_good.ifml");
    assert!(
        params.diagnostics.is_empty(),
        "declared module use should produce no diagnostics, got: {:?}",
        params
            .diagnostics
            .iter()
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );

    do_shutdown(&client_conn);
}

#[test]
fn test_lsp_diagnostic_new_syntax_parses_clean() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
    let (server_conn, client_conn) = Connection::memory();

    std::thread::spawn(move || {
        run_lsp_server(server_conn, GrafeoState::default()).unwrap();
    });

    do_init_handshake(&client_conn);

    // if guards, event conditions, use statements, actor declarations,
    // roles/messages arrays and param defaults must not yield syntax ERRORs
    open_document(
        &client_conn,
        "file:///new_syntax.ifml",
        r#"actor "Manager" { label: "mgr"; }

view "A" {
    roles: [manager];
    messages: ["Welcome"];

    if data.enabled;

    component "c" {
        type: list;

        if count > 0;

        on select(row) if row.active -> navigate("A");
    }
}"#,
    );

    let params = recv_diagnostics(&client_conn, "file:///new_syntax.ifml");
    let syntax_errors: Vec<&Diagnostic> = params
        .diagnostics
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    assert!(
        syntax_errors.is_empty(),
        "new pipeline syntax should parse without ERROR diagnostics, got: {:?}",
        syntax_errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );

    do_shutdown(&client_conn);
}

#[test]
fn test_lsp_completion_view_body_new_keywords() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
    let (server_conn, client_conn) = Connection::memory();

    std::thread::spawn(move || {
        run_lsp_server(server_conn, GrafeoState::default()).unwrap();
    });

    do_init_handshake(&client_conn);

    // Empty line inside the view body (line 4), outside the component
    open_document(
        &client_conn,
        "file:///view_kw.ifml",
        "view \"A\" {\n    component \"c\" {\n        type: list;\n    }\n\n}",
    );

    let _ = recv_diagnostics(&client_conn, "file:///view_kw.ifml");

    client_conn
        .sender
        .send(Message::Request(Request {
            id: RequestId::from(2i32),
            method: "textDocument/completion".to_string(),
            params: serde_json::json!({
                "textDocument": { "uri": "file:///view_kw.ifml" },
                "position": { "line": 4, "character": 0 }
            }),
        }))
        .unwrap();

    let msg = client_conn.receiver.recv().unwrap();
    match msg {
        Message::Response(resp) => {
            let result = resp.result.unwrap_or(serde_json::Value::Null);
            assert!(!result.is_null(), "completion should return results");
            let completion: CompletionResponse = serde_json::from_value(result).unwrap();
            match completion {
                CompletionResponse::List(list) => {
                    let labels: Vec<&str> = list.items.iter().map(|i| i.label.as_str()).collect();
                    for expected in ["if", "use", "roles"] {
                        assert!(
                            labels.contains(&expected),
                            "view body should suggest '{expected}', got: {labels:?}"
                        );
                    }
                }
                _ => panic!("Expected completion list"),
            }
        }
        _ => panic!("Expected completion response"),
    }

    do_shutdown(&client_conn);
}

fn write_policy_fixture(dir: &std::path::Path) {
    std::fs::write(
        dir.join("domain.mox"),
        "package example\n\nclass Ticket {\n    id readonly String ticketNo\n}\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("policy.actor"),
        concat!(
            "import \"domain.mox\"\n",
            "\n",
            "actors Ops {\n",
            "    actor Manager\n",
            "    capability ViewDashboards on Ticket\n",
            "\n",
            "    grant Manager {\n",
            "        permit ViewDashboards\n",
            "    }\n",
            "}\n",
        ),
    )
    .unwrap();
}

fn temp_file_uri(dir: &std::path::Path, name: &str) -> String {
    Url::from_file_path(dir.join(name))
        .expect("valid file path")
        .to_string()
}

#[test]
fn test_lsp_diagnostic_unknown_capability_and_actor() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    write_policy_fixture(tmp.path());

    let (server_conn, client_conn) = Connection::memory();
    std::thread::spawn(move || {
        run_lsp_server(server_conn, GrafeoState::default()).unwrap();
    });

    do_init_handshake(&client_conn);

    let uri = temp_file_uri(tmp.path(), "app.ifml");
    open_document(
        &client_conn,
        &uri,
        r#"import "policy.actor";

view "Dashboard" {
    roles: [Stranger];
    requires: [NoSuchCap];
}"#,
    );

    let params = recv_diagnostics(&client_conn, &uri);
    assert!(
        params.diagnostics.iter().any(|d| {
            d.message.contains("Unknown capability 'NoSuchCap'")
                && d.severity == Some(DiagnosticSeverity::WARNING)
        }),
        "should warn about unknown capability 'NoSuchCap', got: {:?}",
        params
            .diagnostics
            .iter()
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
    assert!(
        params.diagnostics.iter().any(|d| {
            d.message.contains("Unknown actor 'Stranger'")
                && d.severity == Some(DiagnosticSeverity::WARNING)
        }),
        "should warn about unknown actor 'Stranger', got: {:?}",
        params
            .diagnostics
            .iter()
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );

    do_shutdown(&client_conn);
}

#[test]
fn test_lsp_known_capability_and_actor_stay_clean() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    write_policy_fixture(tmp.path());

    let (server_conn, client_conn) = Connection::memory();
    std::thread::spawn(move || {
        run_lsp_server(server_conn, GrafeoState::default()).unwrap();
    });

    do_init_handshake(&client_conn);

    let uri = temp_file_uri(tmp.path(), "app.ifml");
    open_document(
        &client_conn,
        &uri,
        r#"import "policy.actor";

view "Dashboard" {
    roles: [Manager];
    requires: [ViewDashboards];
}"#,
    );

    let params = recv_diagnostics(&client_conn, &uri);
    assert!(
        params.diagnostics.is_empty(),
        "known capability and actor should produce no diagnostics, got: {:?}",
        params
            .diagnostics
            .iter()
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );

    do_shutdown(&client_conn);
}

#[test]
fn test_lsp_unresolvable_import_no_diagnostics_storm() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();

    let (server_conn, client_conn) = Connection::memory();
    std::thread::spawn(move || {
        run_lsp_server(server_conn, GrafeoState::default()).unwrap();
    });

    do_init_handshake(&client_conn);

    // missing.actor does not exist: no policy resolves, so roles/requires
    // must stay undiagnosed instead of flooding with warnings.
    let uri = temp_file_uri(tmp.path(), "app.ifml");
    open_document(
        &client_conn,
        &uri,
        r#"import "missing.actor";

view "Dashboard" {
    roles: [Stranger];
    requires: [NoSuchCap];
}"#,
    );

    let params = recv_diagnostics(&client_conn, &uri);
    assert!(
        !params.diagnostics.iter().any(
            |d| d.message.contains("Unknown actor") || d.message.contains("Unknown capability")
        ),
        "unresolvable import must not produce capability/role diagnostics, got: {:?}",
        params
            .diagnostics
            .iter()
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );

    do_shutdown(&client_conn);
}

fn send_update_positions(
    client: &Connection,
    id: i32,
    uri: &str,
    positions: Vec<serde_json::Value>,
) -> Message {
    client
        .sender
        .send(Message::Request(Request {
            id: RequestId::from(id),
            method: "ifml/updatePositions".to_string(),
            params: serde_json::json!({
                "textDocument": { "uri": uri },
                "positions": positions
            }),
        }))
        .unwrap();
    loop {
        let msg = client.receiver.recv().unwrap();
        match &msg {
            Message::Response(resp) if resp.id == RequestId::from(id) => return msg,
            _ => continue,
        }
    }
}

fn apply_workspace_edit(text: &str, edit: &WorkspaceEdit) -> String {
    let mut result = text.to_string();
    if let Some(changes) = &edit.changes {
        let mut all: Vec<TextEdit> = changes.values().flatten().cloned().collect();
        all.sort_by_key(|e| std::cmp::Reverse(e.range.start.line));
        all.sort_by_key(|e| std::cmp::Reverse(e.range.start.character));
        for e in all {
            let start = offset_from_position(text, e.range.start);
            let end = offset_from_position(text, e.range.end);
            result.replace_range(start..end, &e.new_text);
        }
    }
    result
}

fn offset_from_position(text: &str, pos: Position) -> usize {
    let mut offset = 0;
    for _ in 0..pos.line {
        offset += text[offset..]
            .find('\n')
            .map(|i| i + 1)
            .unwrap_or(text.len() - offset);
    }
    offset + pos.character as usize
}

fn did_change_document(client: &Connection, uri: &str, version: i32, text: &str) {
    client
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didChange".to_string(),
            params: serde_json::json!({
                "textDocument": { "uri": uri, "version": version },
                "contentChanges": [{ "text": text }]
            }),
        }))
        .unwrap();
}

#[test]
fn test_lsp_update_positions() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
    let (server_conn, client_conn) = Connection::memory();

    std::thread::spawn(move || {
        run_lsp_server(server_conn, GrafeoState::default()).unwrap();
    });

    do_init_handshake(&client_conn);

    let original = r#"view "Hello" { component "c" { type: list; data: Person; } }"#;
    open_document(&client_conn, "file:///test.ifml", original);
    let _ = recv_diagnostics(&client_conn, "file:///test.ifml");

    // 1. First update: insert a position property
    let msg = send_update_positions(
        &client_conn,
        20,
        "file:///test.ifml",
        vec![serde_json::json!({ "name": "Hello", "x": 120, "y": 240 })],
    );
    let edit: WorkspaceEdit = match msg {
        Message::Response(resp) => {
            serde_json::from_value(resp.result.expect("expected result")).unwrap()
        }
        other => panic!("Expected response, got {:?}", other),
    };
    let edits = edit.changes.as_ref().expect("changes present");
    let texts: Vec<&str> = edits
        .values()
        .flatten()
        .map(|e| e.new_text.as_str())
        .collect();
    assert!(
        texts
            .iter()
            .any(|t| t.contains("position: { x: 120; y: 240 };")),
        "inserted text should contain position property, got {:?}",
        texts
    );

    // Apply the edit via didChange (full sync) — text stays canonical
    let updated = apply_workspace_edit(original, &edit);
    did_change_document(&client_conn, "file:///test.ifml", 2, &updated);
    let _ = recv_diagnostics(&client_conn, "file:///test.ifml");

    // 2. Second update: must REPLACE the existing property (exactly one)
    let msg = send_update_positions(
        &client_conn,
        21,
        "file:///test.ifml",
        vec![serde_json::json!({ "name": "Hello", "x": 300, "y": 400 })],
    );
    let edit2: WorkspaceEdit = match msg {
        Message::Response(resp) => {
            serde_json::from_value(resp.result.expect("expected result")).unwrap()
        }
        other => panic!("Expected response, got {:?}", other),
    };
    let edits2 = edit2.changes.as_ref().expect("changes present");
    assert_eq!(
        edits2.values().flatten().count(),
        1,
        "exactly one edit expected"
    );
    let final_text = apply_workspace_edit(&updated, &edit2);
    assert_eq!(
        final_text.matches("position:").count(),
        1,
        "exactly one position property after replacement, got: {final_text}"
    );
    assert!(
        final_text.contains("position: { x: 300; y: 400 };"),
        "new position should be in text, got: {final_text}"
    );

    do_shutdown(&client_conn);
}

#[test]
fn test_lsp_update_positions_unknown_view() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
    let (server_conn, client_conn) = Connection::memory();

    std::thread::spawn(move || {
        run_lsp_server(server_conn, GrafeoState::default()).unwrap();
    });

    do_init_handshake(&client_conn);

    open_document(
        &client_conn,
        "file:///test.ifml",
        r#"view "Hello" { component "c" { type: list; data: Person; } }"#,
    );
    let _ = recv_diagnostics(&client_conn, "file:///test.ifml");

    let msg = send_update_positions(
        &client_conn,
        30,
        "file:///test.ifml",
        vec![serde_json::json!({ "name": "NoSuchView", "x": 1, "y": 2 })],
    );
    match msg {
        Message::Response(resp) => {
            let edit: WorkspaceEdit =
                serde_json::from_value(resp.result.expect("expected result")).unwrap();
            let changes = edit.changes.expect("changes map present");
            assert!(
                changes.values().all(|v| v.is_empty()),
                "no edits expected for unknown view, got: {changes:?}"
            );
        }
        other => panic!("Expected response, got {:?}", other),
    }

    do_shutdown(&client_conn);
}

#[test]
fn test_lsp_update_positions_malformed_params() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
    let (server_conn, client_conn) = Connection::memory();

    std::thread::spawn(move || {
        run_lsp_server(server_conn, GrafeoState::default()).unwrap();
    });

    do_init_handshake(&client_conn);

    open_document(
        &client_conn,
        "file:///test.ifml",
        r#"view "Hello" { component "c" { type: list; data: Person; } }"#,
    );
    let _ = recv_diagnostics(&client_conn, "file:///test.ifml");

    client_conn
        .sender
        .send(Message::Request(Request {
            id: RequestId::from(40i32),
            method: "ifml/updatePositions".to_string(),
            params: serde_json::json!({
                "textDocument": { "uri": "file:///test.ifml" },
                "positions": "not-an-array"
            }),
        }))
        .unwrap();

    let msg = client_conn.receiver.recv().unwrap();
    match msg {
        Message::Response(resp) => {
            assert!(
                resp.error.is_some(),
                "malformed params should produce an error response"
            );
        }
        other => panic!("Expected response, got {:?}", other),
    }

    do_shutdown(&client_conn);
}
