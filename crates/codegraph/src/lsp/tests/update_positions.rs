use auto_lsp::lsp_server::{Connection, Message, Notification, Request, RequestId};
use auto_lsp::lsp_types::*;

use super::{LSP_TEST_LOCK, do_init_handshake, do_shutdown, open_document, recv_diagnostics};
use crate::lsp::{GrafeoState, run_lsp_server};

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
