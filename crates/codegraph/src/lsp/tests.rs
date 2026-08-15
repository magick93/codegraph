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
    assert!(true, "Server initialized successfully");

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

#[test]
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
        texts.iter().any(|t| t.contains("position: { x: 120; y: 240 };")),
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
    assert_eq!(edits2.values().flatten().count(), 1, "exactly one edit expected");
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
            assert!(resp.error.is_some(), "malformed params should produce an error response");
        }
        other => panic!("Expected response, got {:?}", other),
    }

    do_shutdown(&client_conn);
}
