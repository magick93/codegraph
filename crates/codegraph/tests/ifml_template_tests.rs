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
    generate_svelte_with_mappings(dir, ifml, None).await
}

async fn generate_svelte_with_mappings(
    dir: &Path,
    ifml: &str,
    mappings: Option<&Path>,
) -> std::path::PathBuf {
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
        ifml_components: mappings,
        ifml_design_system: None,
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

    assert!(
        page.contains("<table data-testid=\"grid-table\" data-pagination=\"true\">"),
        "{page}"
    );
    assert!(page.contains("<tr data-testid=\"grid-row\">"), "{page}");
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
        page.contains("<form data-testid=\"editor-form\" on:submit={submit_editor}>"),
        "{page}"
    );
    assert!(
        page.contains(
            "<button type=\"submit\" data-testid=\"editor-submit\" disabled={submitting}>Submit</button>"
        ),
        "{page}"
    );
    assert!(
        page.contains(
            "<input name=\"name\" type=\"text\" required data-validate=\"len(name) > 2\" />"
        ),
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

#[tokio::test]
async fn mapped_component_renders_invocation_instead_of_inline_markup() {
    let dir = tempfile::tempdir().unwrap();
    let mappings = dir.path().join("ifml-components.toml");
    std::fs::write(
        &mappings,
        r#"
[[component]]
kind = "table"
path = "$lib/components/DataTable.svelte"
export = "DataTable"
testids = { root = "data-table", row = "data-row" }

[[component]]
name = "editor"
path = "$lib/components/CustomerForm.svelte"
"#,
    )
    .unwrap();
    let svelte = generate_svelte_with_mappings(dir.path(), SPECFUL_IFML, Some(&mappings)).await;

    let page = read(&svelte, "src/routes/customertable/+page.svelte");
    assert!(
        page.contains("import DataTable from '$lib/components/DataTable.svelte';"),
        "{page}"
    );
    assert!(page.contains("<DataTable"), "{page}");
    assert!(page.contains("data={data.items}"), "{page}");
    assert!(page.contains("fields={['name', 'status']}"), "{page}");
    assert!(page.contains("testid=\"data-table\""), "{page}");
    assert!(page.contains("rowTestid=\"data-row\""), "{page}");
    assert!(
        !page.contains("<table data-pagination"),
        "mapped table must not render inline markup: {page}"
    );

    let form_page = read(&svelte, "src/routes/customeredit/+page.svelte");
    assert!(
        form_page.contains("import CustomerForm from '$lib/components/CustomerForm.svelte';"),
        "{form_page}"
    );
    assert!(form_page.contains("item={data.formData}"), "{form_page}");
    assert!(
        form_page.contains("data-validate=\"len(name) > 2\""),
        "{form_page}"
    );
    assert!(
        form_page.contains("on:submit={submit_editor}"),
        "{form_page}"
    );
    assert!(
        !form_page.contains("<form"),
        "mapped form must not render inline markup: {form_page}"
    );

    // Unmapped components keep the built-in template fallback.
    let dashboard = read(&svelte, "src/routes/dashboard/+page.svelte");
    assert!(dashboard.contains("data-chart-kind=\"bar\""), "{dashboard}");
}

#[tokio::test]
async fn form_view_submit_handler_posts_to_resolved_endpoint() {
    let dir = tempfile::tempdir().unwrap();
    let ifml = r#"
domain "sales" {
    schema "sales";
}

view "CustomerList" {
    label "Customers";
    landmark: true;

    component "grid" {
        type: list;
        data: Customer;
        fields: [name];
    }
}

view "CustomerEdit" {
    params { customerId: Uuid };

    component "editor" {
        type: form;
        data: Customer;

        field name -> input text { required: true; }

        on save -> navigate("CustomerList", {});
    }
}
"#;
    let svelte = generate_svelte(dir.path(), ifml).await;
    let page = read(&svelte, "src/routes/customeredit/+page.svelte");

    assert!(
        page.contains("import { goto } from '$app/navigation';"),
        "{page}"
    );
    assert!(
        page.contains(
            "const response = await fetch(`/api/v1/sales/customer/${params.customerId}`, {"
        ),
        "{page}"
    );
    assert!(page.contains("method: 'PUT'"), "{page}");
    assert!(page.contains("goto(\"/customerlist\")"), "{page}");
    assert!(
        !page.contains("checkValidity"),
        "no-message forms must keep the plain submit handler: {page}"
    );
    assert!(page.contains("on:submit={submit_editor}"), "{page}");
    assert!(
        page.contains("<span class=\"error\" data-testid=\"editor-error\">{formError}</span>"),
        "{page}"
    );
    assert!(
        page.contains(
            "<button type=\"submit\" data-testid=\"editor-submit\" disabled={submitting}>"
        ),
        "{page}"
    );

    let load = read(&svelte, "src/routes/customeredit/+page.ts");
    assert!(
        load.contains(
            "const customerId = url.searchParams.get('customerId') ?? params.customerId;"
        ),
        "{load}"
    );
    assert!(
        load.contains("`/api/v1/sales/customer/${ customerId }`"),
        "{load}"
    );
}

#[tokio::test]
async fn specless_pages_expose_stable_test_selectors() {
    let dir = tempfile::tempdir().unwrap();
    let svelte = generate_svelte(dir.path(), SPECLESS_IFML).await;
    let list = read(&svelte, "src/routes/customerlist/+page.svelte");
    assert!(
        list.contains("<table data-testid=\"grid-table\">"),
        "{list}"
    );
    assert!(
        list.contains("<tr data-testid=\"grid-row\" onclick={() => comp_grid_select(item)}>"),
        "{list}"
    );

    let details = read(&svelte, "src/routes/customerdetail/+page.svelte");
    assert!(
        details.contains("<dl data-testid=\"info-details\">"),
        "{details}"
    );
}

#[tokio::test]
async fn validation_message_renders_with_data_validate() {
    let dir = tempfile::tempdir().unwrap();
    let ifml = r#"
domain "sales" {
    schema "sales";
}

view "CustomerList" {
    label "Customers";
    landmark: true;

    component "grid" {
        type: list;
        data: Customer;
        fields: [name];
    }
}

view "CustomerEdit" {
    params { customerId: Uuid };

    component "editor" {
        type: form;
        data: Customer;

        field title -> input text {
            required: true;
            validations: [len(title) > 2];
            messages: ["Title too short"];
        }

        on save -> navigate("CustomerList", {});
    }
}
"#;
    let svelte = generate_svelte(dir.path(), ifml).await;
    let page = read(&svelte, "src/routes/customeredit/+page.svelte");

    assert!(
        page.contains(
            "<input name=\"title\" type=\"text\" required data-validate=\"len(title) > 2\" data-validate-message=\"Title too short\" />"
        ),
        "{page}"
    );
    assert!(page.contains("if (!form.checkValidity()) {"), "{page}");
    assert!(
        page.contains(
            "formError = invalid?.getAttribute('data-validate-message') ?? 'Please review the highlighted fields';"
        ),
        "{page}"
    );
    assert!(page.contains("data-testid=\"editor-error\""), "{page}");
    assert!(page.contains("data-testid=\"editor-submit\""), "{page}");
}

#[tokio::test]
async fn specless_form_fallback_renders_submit_and_error_testids() {
    let dir = tempfile::tempdir().unwrap();
    let ifml = r#"
domain "sales" {
    schema "sales";
}

view "CustomerForm" {
    component "editor" {
        type: form;
        data: Customer;
        fields: [name, email];
    }
}
"#;
    let svelte = generate_svelte(dir.path(), ifml).await;
    let page = read(&svelte, "src/routes/customerform/+page.svelte");

    assert!(
        page.contains("<form data-testid=\"editor-form\" on:submit={submit_editor}>"),
        "{page}"
    );
    assert!(
        page.contains("<span class=\"error\" data-testid=\"editor-error\">{formError}</span>"),
        "{page}"
    );
    assert!(
        page.contains(
            "<button type=\"submit\" data-testid=\"editor-submit\" disabled={submitting}>Submit</button>"
        ),
        "{page}"
    );
}

const EDIT_SAVE_IFML: &str = r#"
domain "sales" {
    schema "sales";
}

view "CustomerList" {
    label "Customers";
    landmark: true;

    component "grid" {
        type: list;
        data: Customer;
        fields: [name];

        on select(row) -> navigate("CustomerEdit", { customerId: row.id });
    }
}

view "CustomerEdit" {
    params { customerId: Uuid };

    component "editor" {
        type: form;
        data: Customer;

        field name -> input text { required: true; }

        on save -> navigate("CustomerList", {});
    }
}
"#;

#[tokio::test]
async fn mapped_action_control_renders_button_invocation() {
    let dir = tempfile::tempdir().unwrap();
    let mappings = dir.path().join("ifml-components.toml");
    std::fs::write(
        &mappings,
        r#"
[[component]]
role = "action-control"
path = "$lib/components/Button.svelte"
export = "Button"
testids = { root = "ui-button" }
"#,
    )
    .unwrap();
    let svelte = generate_svelte_with_mappings(dir.path(), EDIT_SAVE_IFML, Some(&mappings)).await;
    let page = read(&svelte, "src/routes/customeredit/+page.svelte");

    assert!(
        page.contains("import Button from '$lib/components/Button.svelte';"),
        "{page}"
    );
    assert!(
        page.contains(
            "<Button onclick={submit_editor} disabled={submitting} testid=\"ui-button\">Save</Button>"
        ),
        "{page}"
    );
    assert!(
        !page.contains("<button type=\"submit\""),
        "mapped button must replace the hardcoded fallback: {page}"
    );
}

const MODAL_IFML: &str = r#"
domain "sales" {
    schema "sales";
}

view "CustomerList" {
    label "Customers";
    landmark: true;

    component "grid" {
        type: list;
        data: Customer;
        fields: [name];

        on select(row) -> navigate("CustomerDialog", { customerId: row.id });
    }
}

view "CustomerDialog" {
    params { customerId: Uuid };
    modal: true;

    component "editor" {
        type: form;
        data: Customer;

        field name -> input text { required: true; }

        on save -> navigate("CustomerList", {});
    }
}
"#;

#[tokio::test]
async fn modal_view_renders_dialog_wrapper_and_nav_passes_dialog_param() {
    let dir = tempfile::tempdir().unwrap();
    let mappings = dir.path().join("ifml-components.toml");
    std::fs::write(
        &mappings,
        r#"
[[component]]
role = "modal-view"
path = "$lib/components/Dialog.svelte"
export = "Dialog"
testids = { root = "customer-modal" }

[[component]]
role = "action-control"
path = "$lib/components/Button.svelte"
export = "Button"
"#,
    )
    .unwrap();
    let svelte = generate_svelte_with_mappings(dir.path(), MODAL_IFML, Some(&mappings)).await;

    let dialog = read(&svelte, "src/routes/customerdialog/+page.svelte");
    assert!(
        dialog.contains("import Dialog from '$lib/components/Dialog.svelte';"),
        "{dialog}"
    );
    assert!(
        dialog.contains("<Dialog bind:open={dialog_open} testid=\"customer-modal\">"),
        "{dialog}"
    );
    assert!(dialog.contains("</Dialog>"), "{dialog}");
    assert!(
        dialog.contains("let dialog_open = $state(true);"),
        "{dialog}"
    );
    assert!(
        dialog.contains(
            "data-testid=\"customerdialog-modal-close\" onclick={close_dialog}>Close</button>"
        ),
        "{dialog}"
    );
    assert!(
        dialog.contains(
            "<Button onclick={submit_editor} disabled={submitting} testid=\"editor-submit\">Save</Button>"
        ),
        "{dialog}"
    );

    let list = read(&svelte, "src/routes/customerlist/+page.svelte");
    assert!(
        list.contains("goto(`/customerdialog?customerId=${row.id}&dialog=open`)"),
        "{list}"
    );
}

#[tokio::test]
async fn modal_view_without_mappings_renders_as_plain_page() {
    let dir = tempfile::tempdir().unwrap();
    let svelte = generate_svelte(dir.path(), MODAL_IFML).await;

    let dialog = read(&svelte, "src/routes/customerdialog/+page.svelte");
    assert!(
        !dialog.contains("dialog_open"),
        "no-pack modal views must not gain dialog state: {dialog}"
    );
    assert!(!dialog.contains("class=\"modal\""), "{dialog}");
    assert!(
        dialog.contains("<form data-testid=\"editor-form\""),
        "{dialog}"
    );
    assert!(
        dialog.contains(
            "<button type=\"submit\" data-testid=\"editor-submit\" disabled={submitting}>Submit</button>"
        ),
        "{dialog}"
    );

    let list = read(&svelte, "src/routes/customerlist/+page.svelte");
    assert!(
        list.contains("goto(`/customerdialog?customerId=${row.id}`)"),
        "{list}"
    );
    assert!(
        !list.contains("dialog=open"),
        "no-pack navigation must keep byte-identical URLs: {list}"
    );
}
