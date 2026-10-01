// ── .mox support (MoxState) ────────────────────────────────────────────

use auto_lsp::lsp_server::{Connection, Message, Notification, Request, RequestId};
use auto_lsp::lsp_types::*;

use super::{do_shutdown, parse_init_result, recv_diagnostics, LSP_TEST_LOCK};
use crate::lsp::mox::build_mox_state;
use crate::lsp::{run_lsp_server, GrafeoState, MoxState};

const MOX_DOMAIN_SOURCE: &str = r#"package nz.example.shop

import schema "schemas/person.json" as Person

class Customer {
    String name
    refers Order[] orders
}

class Order {
    String id
}

enum Status {
    Active as "Active" = 0
    Inactive as "Inactive" = 1
}

type Email wraps String {
    format "email"
}

vocabulary Currency from "iso:4217" {
    key alpha3
    facet String alpha3
}
"#;

/// A temp .mox workspace: one domain file (classes, enum, datatype,
/// vocabulary, one `import schema ... as Person`) plus the imported JSON
/// schema. Returns the dir (keep alive for the URI) and the built state.
fn mox_workspace() -> (tempfile::TempDir, MoxState) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("schemas")).unwrap();
    std::fs::write(dir.path().join("domain.mox"), MOX_DOMAIN_SOURCE).unwrap();
    std::fs::write(
        dir.path().join("schemas/person.json"),
        r#"{ "title": "Person", "type": "object", "properties": {} }"#,
    )
    .unwrap();
    // The Currency vocabulary's vendored snapshot (unique file — the rex
    // compiler takes the version from the filename).
    std::fs::create_dir_all(dir.path().join("vocab")).unwrap();
    std::fs::write(
        dir.path().join("vocab/iso-4217@2024-01-01.json"),
        r#"{ "vocabulary": "iso:4217", "version": "2024-01-01",
             "entries": [ { "alpha3": "USD" }, { "alpha3": "EUR" } ] }"#,
    )
    .unwrap();
    let titles: std::collections::HashSet<String> = ["Person".to_string()].into_iter().collect();
    let state = build_mox_state(&[dir.path().join("domain.mox")], &titles, "Type");
    (dir, state)
}

fn make_mox_init_params() -> serde_json::Value {
    serde_json::json!({
        "processId": null,
        "capabilities": {},
        "initializationOptions": {
            "perFileParser": { "ifml": "ifml", "mox": "mox" }
        },
        "workspaceFolders": null
    })
}

fn do_init_handshake_with_params(client: &Connection, params: serde_json::Value) {
    client
        .sender
        .send(Message::Request(Request {
            id: RequestId::from(1i32),
            method: "initialize".to_string(),
            params,
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

fn open_mox_document(client: &Connection, uri: &str, text: &str) {
    client
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didOpen".to_string(),
            params: serde_json::json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": "mox",
                    "version": 1,
                    "text": text
                }
            }),
        }))
        .unwrap();
}

fn request_completion(client: &Connection, uri: &str, line: u32, character: u32) -> CompletionList {
    client
        .sender
        .send(Message::Request(Request {
            id: RequestId::from(50i32),
            method: "textDocument/completion".to_string(),
            params: serde_json::json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }),
        }))
        .unwrap();
    let msg = client.receiver.recv().unwrap();
    match msg {
        Message::Response(resp) => {
            let result = resp.result.unwrap_or(serde_json::Value::Null);
            assert!(!result.is_null(), "completion should return results");
            match serde_json::from_value::<CompletionResponse>(result).unwrap() {
                CompletionResponse::List(list) => list,
                other => panic!("Expected completion list, got {other:?}"),
            }
        }
        other => panic!("Expected completion response, got {other:?}"),
    }
}

fn request_hover(client: &Connection, uri: &str, line: u32, character: u32) -> Option<Hover> {
    client
        .sender
        .send(Message::Request(Request {
            id: RequestId::from(51i32),
            method: "textDocument/hover".to_string(),
            params: serde_json::json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }),
        }))
        .unwrap();
    let msg = client.receiver.recv().unwrap();
    match msg {
        Message::Response(resp) => {
            let result = resp.result.unwrap_or(serde_json::Value::Null);
            if result.is_null() {
                return None;
            }
            serde_json::from_value::<Hover>(result).ok()
        }
        other => panic!("Expected hover response, got {other:?}"),
    }
}

#[test]
fn test_mox_build_state_extracts_declarations() {
    let (_dir, state) = mox_workspace();

    let customer = state
        .classes
        .iter()
        .find(|c| c.name == "Customer")
        .expect("Customer class in state");
    assert_eq!(customer.package, "nz.example.shop");
    assert!(
        customer.features.iter().any(|f| f.name == "name"),
        "Customer should list its features"
    );

    assert!(state.enums.iter().any(|e| e.name == "Status"));
    let email = state
        .datatypes
        .iter()
        .find(|d| d.name == "Email")
        .expect("Email datatype in state");
    assert_eq!(email.format.as_deref(), Some("email"));
    assert!(state.vocabularies.iter().any(|v| v.name == "Currency"));

    let person = state
        .import_aliases
        .iter()
        .find(|a| a.alias == "Person")
        .expect("Person import alias in state");
    assert!(
        person.abs_path.ends_with("schemas/person.json"),
        "alias should record the resolved abs path, got {}",
        person.abs_path
    );
    assert_eq!(person.resolved_title.as_deref(), Some("Person"));
}

#[test]
fn test_mox_alias_resolution_replays_wire_alias_refs() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("schemas")).unwrap();
    std::fs::write(
        dir.path().join("a.mox"),
        "package a\n\nimport schema \"schemas/Person.json\" as Person\nimport schema \"schemas/Foo.json\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("schemas/Person.json"),
        r#"{ "title": "Person" }"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("schemas/Foo.json"),
        r#"{ "title": "FooType" }"#,
    )
    .unwrap();

    // Titles as the schema pipeline would have them: exact match first, then
    // alias + type suffix (the wire_alias_refs replay contract).
    let titles: std::collections::HashSet<String> = ["Person".to_string(), "FooType".to_string()]
        .into_iter()
        .collect();
    let state = build_mox_state(&[dir.path().join("a.mox")], &titles, "Type");

    let person = state
        .import_aliases
        .iter()
        .find(|a| a.alias == "Person")
        .expect("Person alias");
    assert_eq!(person.resolved_title.as_deref(), Some("Person"));

    // No `as` alias: the file stem "Foo" matches no exact title but matches
    // Foo + "Type" (the second wire_alias_refs try).
    let foo = state
        .import_aliases
        .iter()
        .find(|a| a.alias == "Foo")
        .expect("Foo alias (file stem)");
    assert_eq!(foo.resolved_title.as_deref(), Some("FooType"));
}

#[test]
fn test_mox_unknown_type_diagnostic() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
    let (dir, state) = mox_workspace();
    super::init_mox(Some(state));
    let (server_conn, client_conn) = Connection::memory();

    std::thread::spawn(move || {
        run_lsp_server(server_conn, GrafeoState::default()).unwrap();
    });

    do_init_handshake_with_params(&client_conn, make_mox_init_params());

    let uri = Url::from_file_path(dir.path().join("app.mox")).unwrap();
    open_mox_document(
        &client_conn,
        uri.as_str(),
        "package app\n\nclass Cart {\n    refers Yolo[] items\n}\n",
    );

    let params = recv_diagnostics(&client_conn, uri.as_str());
    assert!(
        params.diagnostics.iter().any(|d| {
            d.severity == Some(DiagnosticSeverity::ERROR)
                && d.message.contains("unknown type 'Yolo'")
        }),
        "should error on unknown type 'Yolo', got: {:?}",
        params
            .diagnostics
            .iter()
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );

    do_shutdown(&client_conn);
}

#[test]
fn test_mox_valid_document_zero_diagnostics() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
    let (dir, state) = mox_workspace();
    super::init_mox(Some(state));
    let (server_conn, client_conn) = Connection::memory();

    std::thread::spawn(move || {
        run_lsp_server(server_conn, GrafeoState::default()).unwrap();
    });

    do_init_handshake_with_params(&client_conn, make_mox_init_params());

    let uri = Url::from_file_path(dir.path().join("app.mox")).unwrap();
    open_mox_document(
        &client_conn,
        uri.as_str(),
        "package app\n\nclass Cart {\n    String name\n    refers Order[] orders\n    Status status\n    Email email\n}\n",
    );

    let params = recv_diagnostics(&client_conn, uri.as_str());
    assert!(
        params.diagnostics.is_empty(),
        "valid mox document should have zero diagnostics, got: {:?}",
        params
            .diagnostics
            .iter()
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );

    do_shutdown(&client_conn);
}

#[test]
fn test_mox_unknown_import_path_diagnostic() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
    let (dir, state) = mox_workspace();
    super::init_mox(Some(state));
    let (server_conn, client_conn) = Connection::memory();

    std::thread::spawn(move || {
        run_lsp_server(server_conn, GrafeoState::default()).unwrap();
    });

    do_init_handshake_with_params(&client_conn, make_mox_init_params());

    let resolved = dir.path().join("schemas/missing.json");
    let uri = Url::from_file_path(dir.path().join("app.mox")).unwrap();
    open_mox_document(
        &client_conn,
        uri.as_str(),
        "import schema \"schemas/missing.json\" as Missing\n\npackage app\n\nclass Cart {\n    String name\n}\n",
    );

    let params = recv_diagnostics(&client_conn, uri.as_str());
    assert!(
        params.diagnostics.iter().any(|d| {
            d.severity == Some(DiagnosticSeverity::ERROR)
                && d.message.contains(&resolved.display().to_string())
        }),
        "import diagnostic should name the resolved path {}, got: {:?}",
        resolved.display(),
        params
            .diagnostics
            .iter()
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );

    do_shutdown(&client_conn);
}

#[test]
fn test_mox_ambiguous_name_warns_but_is_known() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("a.mox"),
        "package a\n\nclass Dup {\n    String x\n}\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("b.mox"),
        "package b\n\nenum Dup {\n    X as \"X\" = 0\n}\n",
    )
    .unwrap();
    let titles: std::collections::HashSet<String> = std::collections::HashSet::new();
    let state = build_mox_state(
        &[dir.path().join("a.mox"), dir.path().join("b.mox")],
        &titles,
        "Type",
    );
    super::init_mox(Some(state));
    let (server_conn, client_conn) = Connection::memory();

    std::thread::spawn(move || {
        run_lsp_server(server_conn, GrafeoState::default()).unwrap();
    });

    do_init_handshake_with_params(&client_conn, make_mox_init_params());

    let uri = Url::from_file_path(dir.path().join("app.mox")).unwrap();
    open_mox_document(
        &client_conn,
        uri.as_str(),
        "package app\n\nclass Cart {\n    refers Dup[] d\n}\n",
    );

    let params = recv_diagnostics(&client_conn, uri.as_str());
    assert!(
        !params
            .diagnostics
            .iter()
            .any(|d| d.message.contains("unknown type")),
        "ambiguous Dup is still a known name, got: {:?}",
        params
            .diagnostics
            .iter()
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
    assert!(
        params
            .diagnostics
            .iter()
            .any(|d| d.severity == Some(DiagnosticSeverity::WARNING)
                && d.message.contains("Ambiguous type name 'Dup'")),
        "ambiguity must be surfaced as a warning, got: {:?}",
        params
            .diagnostics
            .iter()
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );

    do_shutdown(&client_conn);
}

#[test]
fn test_mox_completion_after_refers() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
    let (dir, state) = mox_workspace();
    super::init_mox(Some(state));
    let (server_conn, client_conn) = Connection::memory();

    std::thread::spawn(move || {
        run_lsp_server(server_conn, GrafeoState::default()).unwrap();
    });

    do_init_handshake_with_params(&client_conn, make_mox_init_params());

    let uri = Url::from_file_path(dir.path().join("app.mox")).unwrap();
    open_mox_document(
        &client_conn,
        uri.as_str(),
        "package app\n\nclass Cart {\n    refers \n}\n",
    );
    let _ = recv_diagnostics(&client_conn, uri.as_str());

    // Cursor right after "refers " (line 3, "    refers " = 12 chars)
    let list = request_completion(&client_conn, uri.as_str(), 3, 12);
    let labels: Vec<&str> = list.items.iter().map(|i| i.label.as_str()).collect();
    for expected in ["Customer", "Order", "Status", "Email", "Currency"] {
        assert!(
            labels.contains(&expected),
            "refers completion should include '{expected}', got: {labels:?}"
        );
    }

    do_shutdown(&client_conn);
}

#[test]
fn test_mox_completion_type_position_includes_primitives() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
    let (dir, state) = mox_workspace();
    super::init_mox(Some(state));
    let (server_conn, client_conn) = Connection::memory();

    std::thread::spawn(move || {
        run_lsp_server(server_conn, GrafeoState::default()).unwrap();
    });

    do_init_handshake_with_params(&client_conn, make_mox_init_params());

    let uri = Url::from_file_path(dir.path().join("app.mox")).unwrap();
    open_mox_document(
        &client_conn,
        uri.as_str(),
        "package app\n\nclass Cart {\n    String name\n    Str\n",
    );
    let _ = recv_diagnostics(&client_conn, uri.as_str());

    // Cursor after the partial attribute type "Str" (line 4, char 7)
    let list = request_completion(&client_conn, uri.as_str(), 4, 7);
    let labels: Vec<&str> = list.items.iter().map(|i| i.label.as_str()).collect();
    assert!(
        labels.contains(&"String"),
        "type-position completion should include primitives, got: {labels:?}"
    );
    assert!(
        labels.contains(&"Order"),
        "type-position completion should include model types, got: {labels:?}"
    );

    do_shutdown(&client_conn);
}

#[test]
fn test_mox_hover_class_in_type_position() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
    let (dir, state) = mox_workspace();
    super::init_mox(Some(state));
    let (server_conn, client_conn) = Connection::memory();

    std::thread::spawn(move || {
        run_lsp_server(server_conn, GrafeoState::default()).unwrap();
    });

    do_init_handshake_with_params(&client_conn, make_mox_init_params());

    let uri = Url::from_file_path(dir.path().join("app.mox")).unwrap();
    open_mox_document(
        &client_conn,
        uri.as_str(),
        "package app\n\nclass Cart {\n    refers Customer[] buyers\n}\n",
    );
    let _ = recv_diagnostics(&client_conn, uri.as_str());

    // Hover over "Customer" in the refers type position (line 3, char 14)
    let hover = request_hover(&client_conn, uri.as_str(), 3, 14).expect("hover on Customer");
    let HoverContents::Markup(markup) = hover.contents else {
        panic!("Expected markup hover");
    };
    assert!(
        markup.value.contains("Customer") && markup.value.contains("class"),
        "hover should name the class, got: {}",
        markup.value
    );
    assert!(
        markup.value.contains("nz.example.shop"),
        "hover should show the package, got: {}",
        markup.value
    );
    assert!(
        markup.value.contains("name"),
        "class hover should list features, got: {}",
        markup.value
    );

    do_shutdown(&client_conn);
}

/// Degradation contract (pinned): without `--mox-files` (MoxState absent) a
/// syntactically valid .mox document gets ZERO diagnostics — semantic checks
/// (unknown types, import paths) stay quiet.
#[test]
fn test_mox_quiet_without_state() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    super::init_mox(None);
    let (server_conn, client_conn) = Connection::memory();

    std::thread::spawn(move || {
        run_lsp_server(server_conn, GrafeoState::default()).unwrap();
    });

    do_init_handshake_with_params(&client_conn, make_mox_init_params());

    let uri = Url::from_file_path(dir.path().join("app.mox")).unwrap();
    open_mox_document(
        &client_conn,
        uri.as_str(),
        "package app\n\nclass Cart {\n    refers Yolo[] items\n}\n",
    );

    let params = recv_diagnostics(&client_conn, uri.as_str());
    assert!(
        params.diagnostics.is_empty(),
        "without MoxState the document should be fully quiet, got: {:?}",
        params
            .diagnostics
            .iter()
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );

    do_shutdown(&client_conn);
}

/// Degradation contract (pinned): even without MoxState, tree-sitter syntax
/// errors are still reported (parser-level, needs no model).
#[test]
fn test_mox_syntax_error_reported_without_state() {
    let _lock = LSP_TEST_LOCK.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    super::init_mox(None);
    let (server_conn, client_conn) = Connection::memory();

    std::thread::spawn(move || {
        run_lsp_server(server_conn, GrafeoState::default()).unwrap();
    });

    do_init_handshake_with_params(&client_conn, make_mox_init_params());

    let uri = Url::from_file_path(dir.path().join("app.mox")).unwrap();
    open_mox_document(&client_conn, uri.as_str(), "class Broken {\n");

    let params = recv_diagnostics(&client_conn, uri.as_str());
    assert!(
        params
            .diagnostics
            .iter()
            .any(|d| d.severity == Some(DiagnosticSeverity::ERROR)),
        "syntax errors should still be reported without MoxState, got: {:?}",
        params.diagnostics
    );

    do_shutdown(&client_conn);
}
