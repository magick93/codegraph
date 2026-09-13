use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::{ModuleUseRecord, ViewContainerNode};

fn container<'a>(containers: &'a [ViewContainerNode], name: &str) -> &'a ViewContainerNode {
    containers
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("{name} container should be ingested"))
}

#[tokio::test]
async fn test_module_use_round_trips_through_graph() {
    let ifml = r#"
module "Pagination" {
    input { page: Int, pageSize: Int }
    output { rows: String }

    component "pager" {
        type: list;
        data: Item;
    }
}

view "Catalog" {
    use "Pagination" as pager;

    container "Sidebar" {
        use "Footer" {
            sticky: true;
        }

        component "nav" {
            type: menu;
        }
    }

    component "grid" {
        type: list;
        data: Product;
        fields: [name, price];
    }
}

view "Plain" {
    component "grid" {
        type: list;
        data: Product;
    }
}
"#;
    let engine = codegraph_grafeo::GrafeoEngine::in_memory().expect("in-memory Grafeo engine");
    let model = codegraph_ifml_dsl::parse_ifml(ifml).expect("Should parse IFML with module uses");
    assert_eq!(model.views[0].module_uses.len(), 1);
    assert_eq!(model.views[0].containers[0].module_uses.len(), 1);

    codegraph::ingest::ifml_ingest::ingest_ifml_model(&engine, &model)
        .await
        .expect("Should ingest");

    let containers = engine.get_ifml_view_containers().await.unwrap();
    assert_eq!(containers.len(), 3);

    let catalog = container(&containers, "Catalog");
    assert_eq!(
        catalog.module_uses,
        Some(vec![ModuleUseRecord {
            module: "Pagination".to_string(),
            alias: Some("pager".to_string()),
        }])
    );

    let sidebar = container(&containers, "Sidebar");
    assert_eq!(
        sidebar.module_uses,
        Some(vec![ModuleUseRecord {
            module: "Footer".to_string(),
            alias: None,
        }])
    );

    let plain = container(&containers, "Plain");
    assert_eq!(plain.module_uses, None);
}

#[tokio::test]
async fn test_form_messages_and_param_defaults_ingest_cleanly() {
    let ifml = r#"
view "EditCustomer" {
    params { id: Uuid, tab: String = "details", page: Int = 1 };

    component "form" {
        type: form;
        data: Customer;

        field name  -> input text  { required: true; validations: [len(name) > 2]; messages: ["Name too short"]; }
        field email -> input email;
    }
}
"#;
    let engine = codegraph_grafeo::GrafeoEngine::in_memory().expect("in-memory Grafeo engine");
    let model = codegraph_ifml_dsl::parse_ifml(ifml).expect("Should parse IFML");

    let params = &model.views[0].params;
    assert_eq!(params[0].default, None, "param without default stays None");
    assert_eq!(
        params[1].default,
        Some(codegraph_ifml_dsl::ValueExpression::String(
            "details".to_string()
        ))
    );
    assert_eq!(
        params[2].default,
        Some(codegraph_ifml_dsl::ValueExpression::Number(1.0))
    );

    let fields = match &model.views[0].components[0].spec {
        Some(codegraph_ifml_dsl::ComponentSpec::Form(spec)) => &spec.fields,
        other => panic!("Expected Form spec, got {:?}", other),
    };
    assert_eq!(fields[0].messages, vec!["Name too short".to_string()]);
    assert!(fields[1].messages.is_empty());

    codegraph::ingest::ifml_ingest::ingest_ifml_model(&engine, &model)
        .await
        .expect("Should ingest");

    let containers = engine.get_ifml_view_containers().await.unwrap();
    let view = container(&containers, "EditCustomer");
    assert_eq!(view.module_uses, None);

    let components = engine
        .get_ifml_view_components("EditCustomer")
        .await
        .unwrap();
    assert_eq!(components.len(), 1);
    assert_eq!(components[0].name, "form");
}
