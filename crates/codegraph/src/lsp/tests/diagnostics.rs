use auto_lsp::lsp_server::Connection;
use auto_lsp::lsp_types::*;

use super::{LSP_TEST_LOCK, do_init_handshake, do_shutdown, open_document, recv_diagnostics};
use crate::lsp::{GrafeoState, SchemaInfo, run_lsp_server};

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
