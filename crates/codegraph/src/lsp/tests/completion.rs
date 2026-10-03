use auto_lsp::lsp_server::{Connection, Message, Request, RequestId};
use auto_lsp::lsp_types::*;

use super::{LSP_TEST_LOCK, do_init_handshake, do_shutdown, open_document, recv_diagnostics};
use crate::lsp::{GrafeoState, SchemaInfo, run_lsp_server};

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
