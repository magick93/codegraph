use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::{ModuleUseRecord, ViewContainerNode};
use codegraph_generate::ifml::querier::{IfmlGraphQuerier, IfmlQuerier};

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
async fn test_view_roles_round_trip_through_graph() {
    let ifml = r#"
actor "Admin" {
    label: "Administrator";
}

actor "Auditor" {
    label: "Auditor";
}

view "AdminConsole" {
    roles: [admin, manager];

    component "grid" {
        type: list;
        data: Product;
    }
}

view "Storefront" {
    component "grid" {
        type: list;
        data: Product;
    }
}
"#;
    let engine = codegraph_grafeo::GrafeoEngine::in_memory().expect("in-memory Grafeo engine");
    let model = codegraph_ifml_dsl::parse_ifml(ifml).expect("Should parse IFML with actors/roles");
    assert_eq!(model.actors.len(), 2);
    assert_eq!(model.views[0].roles, vec!["admin", "manager"]);
    assert!(
        !model.views[0].properties.iter().any(|p| p.key == "roles"),
        "roles must not remain in the property bag"
    );

    let stats = codegraph::ingest::ifml_ingest::ingest_ifml_model(&engine, &model)
        .await
        .expect("Should ingest");
    assert_eq!(stats.actors, 2, "actor declarations should be counted");

    let containers = engine.get_ifml_view_containers().await.unwrap();
    assert_eq!(containers.len(), 2);

    let console = container(&containers, "AdminConsole");
    assert_eq!(
        console.roles,
        Some(vec!["admin".to_string(), "manager".to_string()])
    );

    let storefront = container(&containers, "Storefront");
    assert_eq!(storefront.roles, None);
}

#[tokio::test]
async fn test_nested_container_round_trips_as_view_tree() {
    let ifml = r#"
view "Dashboard" {
    label "Dashboard";
    landmark: true;

    container "Sidebar" {
        default: true;

        component "nav" {
            type: menu;
        }
    }

    component "grid" {
        type: list;
        data: Product;
        fields: [name];
    }
}
"#;
    let engine = codegraph_grafeo::GrafeoEngine::in_memory().expect("in-memory Grafeo engine");
    let model = codegraph_ifml_dsl::parse_ifml(ifml).expect("Should parse IFML");
    codegraph::ingest::ifml_ingest::ingest_ifml_model(&engine, &model)
        .await
        .expect("Should ingest");

    let querier = IfmlGraphQuerier::new(&engine);
    let containers = querier.get_view_containers().await.unwrap();
    let mut top_names: Vec<&str> = containers.iter().map(|c| c.name.as_str()).collect();
    top_names.sort();
    assert_eq!(
        top_names,
        vec!["Dashboard"],
        "the nested container must not surface as a top-level view container: {top_names:?}"
    );

    let dashboard = &containers[0];
    assert_eq!(
        dashboard
            .components
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        vec!["grid"],
        "view-level components stay on the view"
    );
    assert_eq!(dashboard.containers.len(), 1, "{dashboard:?}");
    let sidebar = &dashboard.containers[0];
    assert_eq!(sidebar.name, "Sidebar");
    assert!(sidebar.is_default, "container default flag must round-trip");
    assert_eq!(
        sidebar
            .components
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        vec!["nav"],
        "the container's component is queryable under the parent view's tree"
    );
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
