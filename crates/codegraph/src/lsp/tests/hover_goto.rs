use auto_lsp::lsp_server::{Connection, Message, Request, RequestId};
use auto_lsp::lsp_types::*;

use super::{LSP_TEST_LOCK, do_init_handshake, do_shutdown, open_document, recv_diagnostics};
use crate::lsp::{GrafeoState, SchemaInfo, run_lsp_server};

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
