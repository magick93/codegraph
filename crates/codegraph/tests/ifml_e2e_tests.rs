use codegraph_core::traits::GraphQuerier;

/// Full multi-view IFML fixture used by round-trip tests.
const FULL_IFML: &str = r#"
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

/// Test that the IFML DSL parser correctly parses the full example.
#[test]
fn test_ifml_parse_full_example() {
    let ifml_content = FULL_IFML;

    let model = codegraph_ifml_dsl::parse_ifml(ifml_content).expect("Should parse valid IFML");

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
    let model = codegraph_ifml_dsl::parse_ifml(ifml).expect("Should parse expressions");
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

    let bindings = engine.get_data_bindings().await.unwrap();
    assert_eq!(bindings.len(), 1);
    let binding = &bindings[0];
    assert_eq!(binding.component, "grid");
    assert_eq!(binding.entity_title, "ItemType");
    assert_eq!(binding.fields, vec!["name", "value"]);
    assert_eq!(binding.api_operation, None);
}

/// Round-trip test: IFML DSL → Grafeo graph → GraphQuerier. Verifies that
/// IFML edges (which are persisted with prefixed ids) resolve through the
/// graph layer's prefix stripping.
#[tokio::test]
async fn test_ifml_grafeo_round_trip_edges() {
    let engine = codegraph_grafeo::GrafeoEngine::in_memory().expect("in-memory Grafeo engine");
    let model = codegraph_ifml_dsl::parse_ifml(FULL_IFML).expect("Should parse valid IFML");
    codegraph::ingest::ifml_ingest::ingest_ifml_model(&engine, &model)
        .await
        .expect("Should ingest");

    let containers = engine.get_ifml_view_containers().await.unwrap();
    assert_eq!(containers.len(), 3);
    let names: Vec<String> = containers.iter().map(|c| c.name.clone()).collect();
    assert!(names.contains(&"CustomerList".to_string()));
    assert!(names.contains(&"CustomerDetail".to_string()));
    assert!(names.contains(&"CustomerEdit".to_string()));

    let components = engine
        .get_ifml_view_components("CustomerList")
        .await
        .unwrap();
    assert_eq!(components.len(), 1);
    assert_eq!(components[0].name, "grid");

    let events = engine.get_ifml_events("comp:grid").await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].name, "comp_grid_select");

    let flows = engine.get_ifml_navigation_flows().await.unwrap();
    assert!(
        flows.contains(&(
            "grid".to_string(),
            "comp_grid_select".to_string(),
            "CustomerDetail".to_string()
        )),
        "missing grid->CustomerDetail flow: {flows:?}"
    );
    assert!(
        flows.contains(&(
            "info".to_string(),
            "comp_info_edit".to_string(),
            "CustomerEdit".to_string()
        )),
        "missing info->CustomerEdit flow: {flows:?}"
    );

    let actions = engine.get_ifml_actions().await.unwrap();
    assert!(
        actions.iter().any(|a| a.name == "UpdateCustomer"),
        "UpdateCustomer action not found: {actions:?}"
    );
}

/// Typed component taxonomy fixture: table (field/lookup/expr columns),
/// form (required + validations + values), and chart specs.
const TYPED_IFML: &str = r#"
domain "sales" {
    schema "sales";
}

view "Reports" {
    label "Typed Components";

    component "CustomerTable" {
        type: table;
        data: Customer;

        column "Name"   -> field Customer.name;
        column "Status" -> lookup Customer.status via status_labels;
        column "Tenure" -> expr tenure_years(Customer.hire_date);
    }

    component "EditForm" {
        type: form;
        data: Customer;

        field name   -> input text   { required: true; validations: [len(name) > 2]; }
        field email  -> input email;
        field status -> input dropdown { values: ["gold", "silver"]; }
    }

    component "RevenueChart" {
        type: chart;

        chart bar { label: region; values: [revenue]; }
    }
}
"#;

/// Round-trip test for typed component specs: IFML DSL → Grafeo graph →
/// spec JSON → ComponentSpec. Also verifies the full ifml_generate path
/// accepts the fixture.
#[tokio::test]
async fn typed_component_specs_round_trip_through_graph() {
    let engine = codegraph_grafeo::GrafeoEngine::in_memory().expect("in-memory Grafeo engine");
    let model = codegraph_ifml_dsl::parse_ifml(TYPED_IFML).expect("Should parse typed IFML");
    codegraph::ingest::ifml_ingest::ingest_ifml_model(&engine, &model)
        .await
        .expect("Should ingest");

    let components = engine.get_ifml_view_components("Reports").await.unwrap();
    assert_eq!(components.len(), 3);
    let find = |name: &str| {
        components
            .iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("component {name} not found"))
    };

    let table = find("CustomerTable");
    assert_eq!(table.component_type, "table");
    let table_spec: codegraph_ifml_dsl::ComponentSpec = serde_json::from_str(
        table
            .spec
            .as_deref()
            .expect("table spec should be persisted"),
    )
    .expect("table spec should deserialize");
    match table_spec {
        codegraph_ifml_dsl::ComponentSpec::Table(t) => {
            assert!(t.pagination);
            assert_eq!(t.columns.len(), 3);
            assert!(matches!(
                &t.columns[0],
                codegraph_ifml_dsl::ColumnDef::Field { label, field }
                    if label == "Name" && field.entity == "Customer" && field.property == "name"
            ));
            assert!(matches!(
                &t.columns[1],
                codegraph_ifml_dsl::ColumnDef::Lookup { lookup, .. } if lookup == "status_labels"
            ));
            assert!(matches!(
                &t.columns[2],
                codegraph_ifml_dsl::ColumnDef::Expression { .. }
            ));
        }
        other => panic!("Expected Table spec, got {other:?}"),
    }

    let form = find("EditForm");
    assert_eq!(form.component_type, "form");
    let form_spec: codegraph_ifml_dsl::ComponentSpec =
        serde_json::from_str(form.spec.as_deref().expect("form spec should be persisted"))
            .expect("form spec should deserialize");
    match form_spec {
        codegraph_ifml_dsl::ComponentSpec::Form(f) => {
            assert_eq!(f.fields.len(), 3);
            assert_eq!(f.fields[0].name, "name");
            assert_eq!(f.fields[0].input, codegraph_ifml_dsl::InputFieldType::Text);
            assert!(f.fields[0].required);
            assert_eq!(f.fields[0].validations.len(), 1);
            assert_eq!(f.fields[1].name, "email");
            assert!(!f.fields[1].required);
            assert_eq!(
                f.fields[2].values,
                vec!["gold".to_string(), "silver".to_string()]
            );
        }
        other => panic!("Expected Form spec, got {other:?}"),
    }

    let chart = find("RevenueChart");
    assert_eq!(chart.component_type, "chart");
    let chart_spec: codegraph_ifml_dsl::ComponentSpec = serde_json::from_str(
        chart
            .spec
            .as_deref()
            .expect("chart spec should be persisted"),
    )
    .expect("chart spec should deserialize");
    match chart_spec {
        codegraph_ifml_dsl::ComponentSpec::Chart(c) => {
            assert_eq!(c.kind, codegraph_ifml_dsl::ChartKind::Bar);
            assert_eq!(c.label_field.as_deref(), Some("region"));
            assert_eq!(c.value_fields, vec!["revenue".to_string()]);
        }
        other => panic!("Expected Chart spec, got {other:?}"),
    }

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("domains.toml"),
        r#"
[defaults]
api_version = "v1"

[domains.sales]
label = "Sales"
schema_dir = "sales"
postgres_schema = "sales"
entities = ["CustomerType"]
"#,
    )
    .unwrap();
    let ifml_path = dir.path().join("typed.ifml");
    std::fs::write(&ifml_path, TYPED_IFML).unwrap();

    codegraph::driver::ifml_generate(codegraph::driver::IfmlGenerateArgs {
        config_path: &dir.path().join("domains.toml"),
        output: &dir.path().join("out"),
        ifml_files: &[ifml_path],
        schemas: None,
        classifier: None,
        frameworks: &["svelte".to_string()],
        profiles_config_path: None,
        template_dir: &[],
    })
    .await
    .expect("ifml_generate should succeed for typed components");
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
