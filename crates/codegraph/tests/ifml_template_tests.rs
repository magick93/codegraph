//! IFML svelte template generation tests: typed component specs (Table /
//! Form / Chart) plus the byte-identical regression gate for spec-less
//! models (hr-specs regenerates against these templates).

#[path = "fixtures/ifml_template_expected.rs"]
mod expected;

use std::path::Path;

const SPECLESS_IFML: &str = r#"
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
    }
}
"#;

const SPECFUL_IFML: &str = r#"
domain "sales" {
    schema "sales";
}

view "CustomerTable" {
    label "Customers";

    component "grid" {
        type: table;
        data: Customer;
        pagination: true;

        column "Name"   -> field Customer.name;
        column "Status" -> lookup Customer.status via status_labels;
        column "Tenure" -> expr tenure_years(Customer.hire_date);
    }
}

view "CustomerEdit" {
    component "editor" {
        type: form;
        data: Customer;

        field name   -> input text   { required: true; validations: [len(name) > 2]; }
        field email  -> input email;
        field start  -> input datetime;
        field bio    -> input textarea;
        field active -> input toggle;
        field tier   -> input dropdown { values: ["gold", "silver"]; }
        field plan   -> input radio { values: [basic, pro]; }
    }
}

view "Dashboard" {
    component "revenue" {
        type: chart;
        chart bar { label: region; values: [revenue, cost]; }
    }
}
"#;

fn write_domains_toml(dir: &Path) {
    std::fs::write(
        dir.join("domains.toml"),
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
}

async fn generate_svelte(dir: &Path, ifml: &str) -> std::path::PathBuf {
    let ifml_path = dir.join("app.ifml");
    std::fs::write(&ifml_path, ifml).unwrap();
    let output = dir.join("out");
    let domains_toml = dir.join("domains.toml");
    write_domains_toml(dir);

    codegraph::driver::ifml_generate(codegraph::driver::IfmlGenerateArgs {
        config_path: &domains_toml,
        output: &output,
        ifml_files: &[ifml_path],
        schemas: None,
        classifier: None,
        frameworks: &["svelte".to_string()],
        profiles_config_path: None,
        template_dir: &[],
    })
    .await
    .unwrap();

    output.join("svelte")
}

fn read(svelte_dir: &Path, rel: &str) -> String {
    std::fs::read_to_string(svelte_dir.join(rel)).unwrap()
}

#[tokio::test]
async fn specless_svelte_output_is_byte_identical() {
    let dir = tempfile::tempdir().unwrap();
    let svelte = generate_svelte(dir.path(), SPECLESS_IFML).await;

    assert_eq!(
        read(&svelte, "src/routes/customerlist/+page.svelte"),
        expected::SPECLESS_CUSTOMERLIST_PAGE
    );
    assert_eq!(
        read(&svelte, "src/routes/customerlist/+page.ts"),
        expected::SPECLESS_CUSTOMERLIST_LOAD
    );
    assert_eq!(
        read(&svelte, "src/routes/customerdetail/+page.svelte"),
        expected::SPECLESS_CUSTOMERDETAIL_PAGE
    );
    assert_eq!(
        read(&svelte, "src/routes/customerdetail/+page.ts"),
        expected::SPECLESS_CUSTOMERDETAIL_LOAD
    );
    assert_eq!(
        read(&svelte, "src/lib/routes.ts"),
        expected::SPECLESS_ROUTES_TS
    );
    assert_eq!(
        read(&svelte, "src/lib/route-helpers.ts"),
        expected::SPECLESS_ROUTE_HELPERS
    );
}

#[tokio::test]
async fn table_spec_renders_typed_columns() {
    let dir = tempfile::tempdir().unwrap();
    let svelte = generate_svelte(dir.path(), SPECFUL_IFML).await;
    let page = read(&svelte, "src/routes/customertable/+page.svelte");

    assert!(page.contains("<table data-pagination=\"true\">"), "{page}");
    assert!(page.contains("<th>Name</th>"), "{page}");
    assert!(page.contains("<th>Status</th>"), "{page}");
    assert!(page.contains("<th>Tenure</th>"), "{page}");
    assert!(page.contains("<td>{item.name}</td>"), "{page}");
    assert!(page.contains("<td>{item.status}</td>"), "{page}");
    assert!(
        page.contains("<td>{item.tenure_years(Customer.hire_date)}</td>"),
        "{page}"
    );
    assert!(
        !page.contains("<th>name</th>"),
        "legacy field header leaked"
    );
}

#[tokio::test]
async fn form_spec_renders_typed_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let svelte = generate_svelte(dir.path(), SPECFUL_IFML).await;
    let page = read(&svelte, "src/routes/customeredit/+page.svelte");

    assert!(
        page.contains("<input name=\"name\" type=\"text\" required />"),
        "{page}"
    );
    assert!(
        page.contains("<input name=\"email\" type=\"email\" />"),
        "{page}"
    );
    assert!(
        page.contains("<input name=\"start\" type=\"datetime-local\" />"),
        "{page}"
    );
    assert!(
        page.contains("<textarea name=\"bio\"></textarea>"),
        "{page}"
    );
    assert!(
        page.contains("<input name=\"active\" type=\"checkbox\" />"),
        "{page}"
    );
    assert!(page.contains("<select name=\"tier\">"), "{page}");
    assert!(
        page.contains("<option value=\"gold\">gold</option>"),
        "{page}"
    );
    assert!(
        page.contains("<option value=\"silver\">silver</option>"),
        "{page}"
    );
    assert!(page.contains("<legend>plan</legend>"), "{page}");
    assert!(
        page.contains("<input type=\"radio\" name=\"plan\" value=\"basic\" />"),
        "{page}"
    );
    assert!(
        page.contains("<input type=\"radio\" name=\"plan\" value=\"pro\" />"),
        "{page}"
    );
}

#[tokio::test]
async fn chart_spec_renders_placeholder_block() {
    let dir = tempfile::tempdir().unwrap();
    let svelte = generate_svelte(dir.path(), SPECFUL_IFML).await;
    let page = read(&svelte, "src/routes/dashboard/+page.svelte");

    assert!(
        page.contains(
            "<div class=\"chart\" data-chart-kind=\"bar\" data-label-field=\"region\" data-value-fields=\"revenue,cost\"></div>"
        ),
        "{page}"
    );
}
