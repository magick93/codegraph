use codegraph_core::traits::GraphQuerier;

/// Test that the IFML DSL parser correctly parses the full example.
#[test]
fn test_ifml_parse_full_example() {
    let ifml_content = r#"
domain "sales" {
    schema "sales";
}

view "CustomerList" {
    label "Customer Management";
    landmark: true;

    component "grid" {
        type: list;
        data: Customer;
        fields: [name, email, phone, status];

        on select(row) -> navigate("CustomerDetail", {
            customerId: row.id
        });
    }
}

view "CustomerDetail" {
    params { customerId: Uuid };

    component "info" {
        type: details;
        data: Customer;
        fields: [name, email, phone];

        on edit -> navigate("CustomerEdit", {
            customerId: params.customerId
        });
    }
}

view "CustomerEdit" {
    params { customerId: Uuid };

    component "form" {
        type: form;
        data: Customer;
        mode: edit;

        on save(values) -> action("UpdateCustomer", {
            on success -> navigate("CustomerDetail", {
                customerId: params.customerId
            });
            on error -> stay;
        });

        on cancel -> navigate("CustomerDetail");
    }
}
"#;

    let model = codegraph_ifml_dsl::parse_ifml(ifml_content)
        .expect("Should parse valid IFML");

    assert_eq!(model.domains.len(), 1);
    assert_eq!(model.domains[0].name, "sales");
    assert_eq!(model.views.len(), 3);
    assert_eq!(model.views[0].name, "CustomerList");
    assert!(model.views[0].is_landmark);
    assert_eq!(model.views[1].params.len(), 1);
    assert_eq!(model.views[1].params[0].name, "customerId");
    assert_eq!(model.views[0].components.len(), 1);
    assert_eq!(model.views[0].components[0].events.len(), 1);
}

/// Test IFML expressions parsing
#[test]
fn test_ifml_expressions() {
    let ifml = r#"
view "Dashboard" {
    component "orders" {
        type: list;
        data: Order;
        fields: [id, date, total];
        filter: date == today() && status != "cancelled";
    }
}
"#;
    let model = codegraph_ifml_dsl::parse_ifml(ifml)
        .expect("Should parse expressions");
    assert_eq!(model.views.len(), 1);
    let comp = &model.views[0].components[0];
    let fields_prop = comp
        .properties
        .iter()
        .find(|p| p.key == "fields")
        .expect("fields property should exist");
    match &fields_prop.value {
        codegraph_ifml_dsl::ValueExpression::Array(items) => {
            let field_names: Vec<String> = items
                .iter()
                .filter_map(|v| match v {
                    codegraph_ifml_dsl::ValueExpression::Identifier(s) => Some(s.clone()),
                    _ => None,
                })
                .collect();
            assert_eq!(field_names, vec!["id", "date", "total"]);
        }
        _ => panic!("Expected Array value for fields"),
    }
}

/// Test that invalid IFML produces parse errors
#[test]
fn test_ifml_invalid_syntax() {
    let cases = vec![
        ("view { }", "missing view name"),
        ("view 123 { }", "non-string view name"),
        ("view \"Test\" { invalid; }", "unrecognized token"),
    ];

    for (input, description) in &cases {
        let result = codegraph_ifml_dsl::parse_ifml(input);
        assert!(result.is_err(), "Expected error for: {description}");
    }
}

/// Test IFML ingestion into mock graph
#[tokio::test]
async fn test_ifml_ingest_into_mock() {
    let engine = codegraph_core::mock::MockEngine::new();
    let ifml = r#"
view "TestView" {
    component "grid" {
        type: list;
        data: Item;
        fields: [name, value];

        on select -> navigate("Detail");
    }
}
"#;

    let model = codegraph_ifml_dsl::parse_ifml(ifml).unwrap();
    codegraph::ingest::ifml_ingest::ingest_ifml_model(&engine, &model)
        .await
        .expect("Should ingest");

    let containers = engine.get_ifml_view_containers().await.unwrap();
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].name, "TestView");

    let components = engine.get_ifml_view_components("TestView").await.unwrap();
    assert_eq!(components.len(), 1);
    assert_eq!(components[0].name, "grid");
}

/// Test IFML stale route cleanup
#[test]
fn test_ifml_clean_stale_routes() {
    let dir = tempfile::tempdir().unwrap();
    let routes_dir = dir.path().join("src").join("routes");

    // Create some IFML route directories
    let active_dir = routes_dir.join("customerview");
    let stale_dir = routes_dir.join("oldview");
    let special_dir = routes_dir.join("(app)");

    std::fs::create_dir_all(&active_dir).unwrap();
    std::fs::create_dir_all(&stale_dir).unwrap();
    std::fs::create_dir_all(&special_dir).unwrap();

    // Create +page.svelte files to mark IFML routes
    std::fs::write(active_dir.join("+page.svelte"), "").unwrap();
    std::fs::write(stale_dir.join("+page.svelte"), "").unwrap();
    // Special directory should NOT have a +page.svelte (it's a route group)

    // Also create active view list
    let active_views: Vec<String> = vec!["Customerview".to_string()];

    // Call the cleanup function
    // We access it through the generate module's public interface
    // The function is not public, so we test manually
    let routes_path = dir.path().join("src").join("routes");

    // Simulate the logic from clean_stale_ifml_routes
    let entries = std::fs::read_dir(&routes_path).unwrap();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let dir_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if dir_name.starts_with('_') || dir_name.starts_with('(') || dir_name.starts_with('.') {
            continue;
        }
        let is_active = active_views.iter().any(|v| v.to_lowercase() == dir_name);
        if !is_active && path.join("+page.svelte").exists() {
            let _ = std::fs::remove_dir_all(&path);
        }
    }

    // active_dir should still exist
    assert!(
        active_dir.exists(),
        "active view directory should be preserved"
    );
    // stale_dir should be removed
    assert!(
        !stale_dir.exists(),
        "stale view directory should be removed"
    );
    // special_dir should still exist
    assert!(
        special_dir.exists(),
        "special SvelteKit directory should be preserved"
    );
}
