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

fn domains_toml_without_workflow() -> &'static str {
    r#"
[defaults]
api_version = "v1"

[domains.sales]
label = "Sales"
schema_dir = "sales"
postgres_schema = "sales"
entities = ["CustomerType"]
"#
}

fn domains_toml_with_workflow() -> &'static str {
    r#"
[defaults]
api_version = "v1"

[domains.sales]
label = "Sales"
schema_dir = "sales"
postgres_schema = "sales"
entities = ["CustomerType"]

[domains.sales.entity_config.CustomerType.workflow]
status_field = "status"
initial_state = "received"
states = ["received", "review", "done"]
terminal_states = ["done"]
"#
}

async fn generate_svelte(dir: &Path, ifml: &str) -> std::path::PathBuf {
    generate_svelte_with_mappings(dir, ifml, None).await
}

async fn generate_svelte_with_mappings(
    dir: &Path,
    ifml: &str,
    mappings: Option<&Path>,
) -> std::path::PathBuf {
    generate_svelte_with_domains(dir, ifml, domains_toml_without_workflow(), mappings).await
}

async fn generate_svelte_with_domains(
    dir: &Path,
    ifml: &str,
    domains_toml: &str,
    mappings: Option<&Path>,
) -> std::path::PathBuf {
    let ifml_path = dir.join("app.ifml");
    std::fs::write(&ifml_path, ifml).unwrap();
    let output = dir.join("out");
    let domains_toml_path = dir.join("domains.toml");
    std::fs::write(&domains_toml_path, domains_toml).unwrap();

    codegraph::driver::ifml_generate(codegraph::driver::IfmlGenerateArgs {
        config_path: &domains_toml_path,
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
        page.contains("<form data-testid=\"editor-form\" onsubmit={submit_editor}>"),
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
            "<input name=\"name\" type=\"text\" value={editor_form_state.name} required data-validate=\"len(name) > 2\" />"
        ),
        "{page}"
    );
    assert!(
        page.contains("<input name=\"email\" type=\"email\" value={editor_form_state.email} />"),
        "{page}"
    );
    assert!(
        page.contains(
            "<input name=\"start\" type=\"datetime-local\" value={editor_form_state.start} />"
        ),
        "{page}"
    );
    assert!(
        page.contains("<textarea name=\"bio\" value={editor_form_state.bio}></textarea>"),
        "{page}"
    );
    assert!(
        page.contains(
            "<input name=\"active\" type=\"checkbox\" checked={editor_form_state.active === true} />"
        ),
        "{page}"
    );
    assert!(
        page.contains("<select name=\"tier\" value={editor_form_state.tier}>"),
        "{page}"
    );
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
    assert!(page.contains("<h1>Customers</h1>"), "{page}");
    let heading = page.find("<h1>").unwrap();
    let invocation = page.find("<DataTable").unwrap();
    assert!(
        heading < invocation,
        "heading renders above the mapped component: {page}"
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
        form_page.contains("onsubmit={submit_editor}"),
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
            "const viewParams = $derived((data.params ?? {}) as Record<string, string>);"
        ),
        "{page}"
    );
    assert!(
        page.contains(
            "const response = await fetch(`/api/v1/sales/customer/${viewParams.customerId}`, {"
        ),
        "edit submits must read the id from the query-param viewParams: {page}"
    );
    assert!(page.contains("method: 'PUT'"), "{page}");
    assert!(page.contains("goto(\"/customerlist\")"), "{page}");
    assert!(
        !page.contains("checkValidity"),
        "no-message forms must keep the plain submit handler: {page}"
    );
    assert!(page.contains("onsubmit={submit_editor}"), "{page}");
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
        load.contains("const customerId = viewParams['customerId'];"),
        "load resolves params from the query string only: {load}"
    );
    assert!(
        load.contains("`/api/v1/sales/customer/${ customerId }`"),
        "{load}"
    );
}

#[tokio::test]
async fn mapped_table_with_select_event_emits_svelte5_onselect_prop() {
    let dir = tempfile::tempdir().unwrap();
    let mappings = dir.path().join("ifml-components.toml");
    std::fs::write(
        &mappings,
        r#"
[[component]]
kind = "list"
path = "$lib/components/DataTable.svelte"
export = "DataTable"
testids = { root = "data-table", row = "data-row" }
"#,
    )
    .unwrap();
    let svelte = generate_svelte_with_mappings(dir.path(), SPECLESS_IFML, Some(&mappings)).await;
    let page = read(&svelte, "src/routes/customerlist/+page.svelte");

    assert!(
        page.contains("onselect={comp_grid_select}"),
        "row-select handlers must ride the Svelte 5 event-property form so components receive them: {page}"
    );
    assert!(
        !page.contains("on:select"),
        "on:select directives are dropped on components: {page}"
    );
}

#[tokio::test]
async fn form_payload_coerces_typed_values_from_dsl_input_types() {
    let dir = tempfile::tempdir().unwrap();
    let svelte = generate_svelte(dir.path(), SPECFUL_IFML).await;
    let page = read(&svelte, "src/routes/customeredit/+page.svelte");
    assert!(
        page.contains("payload.active = formData.active === 'on';"),
        "checkbox inputs coerce to booleans: {page}"
    );
    assert!(
        page.contains(
            "payload.start = formData.start === '' ? null : new Date(String(formData.start)).toISOString();"
        ),
        "datetime inputs coerce to ISO strings: {page}"
    );
    assert!(
        page.contains("body: JSON.stringify(payload)"),
        "the submit body uses the coerced payload: {page}"
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
            "<input name=\"title\" type=\"text\" value={editor_form_state.title} required data-validate=\"len(title) > 2\" data-validate-message=\"Title too short\" />"
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
        page.contains("<form data-testid=\"editor-form\" onsubmit={submit_editor}>"),
        "{page}"
    );
    assert!(
        page.contains("<input name=\"name\" value={editor_form_state.name} />"),
        "form inputs pre-fill from the loaded entity: {page}"
    );
    assert!(
        page.contains("<input name=\"email\" value={editor_form_state.email} />"),
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

const XOR_IFML: &str = r#"
domain "sales" {
    schema "sales";
}

view "Checkout" {
    label "Checkout";
    xor: true;

    component "editor" {
        type: form;
        data: Customer;

        field name -> input text { required: true; }

        on save -> navigate("CustomerList", {});
    }
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
"#;

#[tokio::test]
async fn xor_container_renders_mapped_card_wrapper_around_children() {
    let dir = tempfile::tempdir().unwrap();
    let mappings = dir.path().join("ifml-components.toml");
    std::fs::write(
        &mappings,
        r#"
[[component]]
role = "presentation-container"
path = "$lib/components/Card.svelte"
export = "Card"
testids = { root = "card" }
"#,
    )
    .unwrap();
    let svelte = generate_svelte_with_mappings(dir.path(), XOR_IFML, Some(&mappings)).await;
    let page = read(&svelte, "src/routes/checkout/+page.svelte");

    assert!(
        page.contains("import Card from '$lib/components/Card.svelte';"),
        "{page}"
    );
    assert!(page.contains("<Card testid=\"card\">"), "{page}");
    assert!(page.contains("</Card>"), "{page}");
    assert!(
        page.contains("<form data-testid=\"editor-form\""),
        "children must render inside the wrapper: {page}"
    );
    let open = page.find("<Card testid=\"card\">").unwrap();
    let child = page.find("<form").unwrap();
    let close = page.rfind("</Card>").unwrap();
    assert!(open < child && child < close, "{page}");
}

#[tokio::test]
async fn xor_container_renders_section_fallback_when_pack_present() {
    let dir = tempfile::tempdir().unwrap();
    let mappings = dir.path().join("ifml-components.toml");
    std::fs::write(
        &mappings,
        r#"
[[component]]
role = "action-control"
path = "$lib/components/Button.svelte"
export = "Button"
"#,
    )
    .unwrap();
    let svelte = generate_svelte_with_mappings(dir.path(), XOR_IFML, Some(&mappings)).await;
    let page = read(&svelte, "src/routes/checkout/+page.svelte");

    assert!(
        page.contains("<section data-testid=\"checkout-container\">"),
        "{page}"
    );
    assert!(page.contains("</section>"), "{page}");
    assert!(!page.contains("<Card"), "{page}");
}

#[tokio::test]
async fn xor_container_without_pack_renders_plain_page() {
    let dir = tempfile::tempdir().unwrap();
    let svelte = generate_svelte(dir.path(), XOR_IFML).await;
    let page = read(&svelte, "src/routes/checkout/+page.svelte");

    assert!(!page.contains("<section"), "{page}");
    assert!(!page.contains("<Card"), "{page}");
    assert!(
        page.contains("<form data-testid=\"editor-form\""),
        "no-pack xor views must render exactly as before: {page}"
    );

    // Landmark page is equally untouched without a pack.
    let list = read(&svelte, "src/routes/customerlist/+page.svelte");
    assert!(
        list.contains("<table data-testid=\"grid-table\">"),
        "{list}"
    );
}

const ROLES_IFML: &str = r#"
domain "sales" {
    schema "sales";
}

view "AdminConsole" {
    label "Admin Console";
    roles: [admin, manager];

    component "grid" {
        type: list;
        data: Customer;
        fields: [name];
    }
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
"#;

#[tokio::test]
async fn role_guarded_view_emits_load_guard_and_roles_helper() {
    let dir = tempfile::tempdir().unwrap();
    let svelte = generate_svelte(dir.path(), ROLES_IFML).await;

    let load = read(&svelte, "src/routes/adminconsole/+page.ts");
    assert!(
        load.contains("import { currentRoles } from '$lib/roles';"),
        "{load}"
    );
    assert!(
        load.contains("import { redirect } from '@sveltejs/kit';"),
        "{load}"
    );
    assert!(
        load.contains("const viewRoles = ['admin', 'manager'];"),
        "{load}"
    );
    assert!(load.contains("const roles = currentRoles();"), "{load}");
    assert!(
        load.contains(
            "if (browser && viewRoles.length && !roles.some((r) => viewRoles.includes(r))) {"
        ),
        "{load}"
    );
    assert!(
        load.contains("throw redirect(303, '/customerlist');"),
        "denial redirects to the first unguarded view: {load}"
    );
    assert!(!load.contains("can("), "{load}");
    let load_start = load.find("export const load").unwrap();
    let guard_start = load.find("const roles = currentRoles();").unwrap();
    assert!(
        load_start < guard_start,
        "guard must sit inside load: {load}"
    );

    let helper = read(&svelte, "src/lib/roles.ts");
    assert!(
        helper.contains("export function currentRoles(): string[] {"),
        "{helper}"
    );
    assert!(
        helper.contains("(globalThis as any).__USER_ROLES__ ?? []"),
        "{helper}"
    );
    assert!(
        !helper.contains("can("),
        "roles-only runs without policy keep the legacy helper shape: {helper}"
    );

    let unguarded = read(&svelte, "src/routes/customerlist/+page.ts");
    assert!(
        !unguarded.contains("currentRoles"),
        "views without roles must stay guard-free: {unguarded}"
    );
}

const REQUIRES_IFML: &str = r#"
domain "sales" {
    schema "sales";
}

view "RefundConsole" {
    label "Refund Console";
    requires: [manage_refunds];

    component "grid" {
        type: list;
        data: Customer;
        fields: [name];
    }
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
"#;

#[tokio::test]
async fn requires_guarded_view_emits_can_checks_and_capability_roles_helper() {
    let dir = tempfile::tempdir().unwrap();
    let svelte = generate_svelte(dir.path(), REQUIRES_IFML).await;

    let load = read(&svelte, "src/routes/refundconsole/+page.ts");
    assert!(
        load.contains("import { redirect } from '@sveltejs/kit';"),
        "{load}"
    );
    assert!(load.contains("import { can } from '$lib/roles';"), "{load}");
    assert!(
        !load.contains("currentRoles"),
        "capability-only views need no roles import: {load}"
    );
    assert!(
        load.contains("const viewRequires = ['manage_refunds'];"),
        "{load}"
    );
    assert!(
        load.contains("if (browser && viewRequires.length && !viewRequires.some((c) => can(c))) {"),
        "{load}"
    );
    assert!(
        load.contains("throw redirect(303, '/customerlist');"),
        "denial redirects to the first unguarded view: {load}"
    );

    let helper = read(&svelte, "src/lib/roles.ts");
    assert!(
        helper.contains("export function can(capability: string): boolean {"),
        "{helper}"
    );
    assert!(
        helper.contains("(globalThis as any).__USER_CAPABILITIES__ ?? []"),
        "{helper}"
    );
    assert!(
        !helper.contains("ROLE_CAPABILITIES"),
        "no ingested policy must mean no embedded capability map: {helper}"
    );
    assert!(
        helper.contains("export function currentRoles(): string[] {"),
        "{helper}"
    );

    let unguarded = read(&svelte, "src/routes/customerlist/+page.ts");
    assert!(
        !unguarded.contains("can("),
        "views without requires must stay guard-free: {unguarded}"
    );
}

const GUARDED_FORM_IFML: &str = r#"
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

view "RefundEdit" {
    component "editor" {
        type: form;
        data: Customer;

        field name -> input text { required: true; }

        on save -> navigate("CustomerList", {});
    }
}
"#;

fn guarded_form_ifml(guard: &str) -> String {
    GUARDED_FORM_IFML.replace(
        "view \"RefundEdit\" {",
        &format!("view \"RefundEdit\" {{\n    {}", guard),
    )
}

#[tokio::test]
async fn requires_view_gates_submit_and_emits_can_import() {
    let dir = tempfile::tempdir().unwrap();
    let svelte = generate_svelte(
        dir.path(),
        &guarded_form_ifml("requires: [manage_refunds];"),
    )
    .await;

    let page = read(&svelte, "src/routes/refundedit/+page.svelte");
    assert!(page.contains("import { can } from '$lib/roles';"), "{page}");
    assert!(
        !page.contains("currentRoles"),
        "capability-only views need no roles import: {page}"
    );
    assert!(
        page.contains("const viewRequires = ['manage_refunds'];"),
        "{page}"
    );
    assert!(
        page.contains(
            "{#if viewRequires.some((c) => can(c))}<button type=\"submit\" data-testid=\"editor-submit\" disabled={submitting}>Submit</button>{/if}"
        ),
        "{page}"
    );

    let list = read(&svelte, "src/routes/customerlist/+page.svelte");
    assert!(
        !list.contains("$lib/roles"),
        "unguarded pages must not import the roles helper: {list}"
    );
    assert!(
        !list.contains("can(") && !list.contains("{#if view"),
        "unguarded pages must stay gate-free: {list}"
    );
}

#[tokio::test]
async fn roles_view_gates_submit_and_emits_current_roles_import() {
    let dir = tempfile::tempdir().unwrap();
    let svelte = generate_svelte(dir.path(), &guarded_form_ifml("roles: [admin, manager];")).await;

    let page = read(&svelte, "src/routes/refundedit/+page.svelte");
    assert!(
        page.contains("import { currentRoles } from '$lib/roles';"),
        "{page}"
    );
    assert!(
        !page.contains("can("),
        "role-only views need no capability import: {page}"
    );
    assert!(
        page.contains("const viewRoles = ['admin', 'manager'];"),
        "{page}"
    );
    assert!(page.contains("const roles = currentRoles();"), "{page}");
    assert!(
        page.contains(
            "{#if roles.some((r) => viewRoles.includes(r))}<button type=\"submit\" data-testid=\"editor-submit\" disabled={submitting}>Submit</button>{/if}"
        ),
        "{page}"
    );
}

#[tokio::test]
async fn combined_guarded_view_gates_submit_with_and_of_checks() {
    let dir = tempfile::tempdir().unwrap();
    let svelte = generate_svelte(
        dir.path(),
        &guarded_form_ifml("roles: [admin];\n    requires: [manage_refunds];"),
    )
    .await;

    let page = read(&svelte, "src/routes/refundedit/+page.svelte");
    assert!(
        page.contains("import { currentRoles } from '$lib/roles';"),
        "{page}"
    );
    assert!(page.contains("import { can } from '$lib/roles';"), "{page}");
    assert!(
        page.contains(
            "{#if viewRequires.some((c) => can(c)) && roles.some((r) => viewRoles.includes(r))}<button type=\"submit\" data-testid=\"editor-submit\" disabled={submitting}>Submit</button>{/if}"
        ),
        "markup must match the load guard's requires-AND-roles semantics: {page}"
    );
}

#[tokio::test]
async fn mapped_action_control_button_gated_for_requires_view() {
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
    let svelte = generate_svelte_with_mappings(
        dir.path(),
        &guarded_form_ifml("requires: [manage_refunds];"),
        Some(&mappings),
    )
    .await;

    let page = read(&svelte, "src/routes/refundedit/+page.svelte");
    assert!(
        page.contains(
            "{#if viewRequires.some((c) => can(c))}<Button onclick={submit_editor} disabled={submitting} testid=\"ui-button\">Save</Button>{/if}"
        ),
        "{page}"
    );
    assert!(
        !page.contains("<button type=\"submit\""),
        "mapped button must replace the fallback: {page}"
    );
}

const SHELL_IFML: &str = r#"
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

        on select(row) -> navigate("CustomerDetail", {
            customerId: row.id
        });
    }
}

view "CustomerDetail" {
    params { customerId: Uuid };
    label "Customer Detail";

    component "info" {
        type: details;
        data: Customer;
        fields: [name];
    }
}
"#;

#[tokio::test]
async fn landmark_shell_emits_layout_with_nav_items() {
    let dir = tempfile::tempdir().unwrap();
    let mappings = dir.path().join("ifml-components.toml");
    std::fs::write(
        &mappings,
        r#"
[[component]]
role = "shell"
path = "$lib/components/Nav.svelte"
export = "Nav"
testids = { root = "side-nav" }
"#,
    )
    .unwrap();
    let svelte = generate_svelte_with_mappings(dir.path(), SHELL_IFML, Some(&mappings)).await;
    let layout = read(&svelte, "src/routes/+layout.svelte");

    assert!(
        layout.contains("import Nav from '$lib/components/Nav.svelte';"),
        "{layout}"
    );
    assert!(
        layout.contains("let { children }: { children: import('svelte').Snippet } = $props();"),
        "{layout}"
    );
    assert!(layout.contains("<Nav testid=\"side-nav\">"), "{layout}");
    assert!(
        layout.contains("<a href={`/customerdetail?customerId=${row.id}`}>Customer Detail</a>"),
        "{layout}"
    );
    assert!(layout.contains("</Nav>"), "{layout}");
    assert!(layout.contains("{@render children()}"), "{layout}");

    // Pages need no changes for the SvelteKit layout convention.
    let list = read(&svelte, "src/routes/customerlist/+page.svelte");
    assert!(
        list.contains("<table data-testid=\"grid-table\">"),
        "{list}"
    );
}

#[tokio::test]
async fn no_shell_mapping_emits_no_layout() {
    let dir = tempfile::tempdir().unwrap();
    let svelte = generate_svelte(dir.path(), SHELL_IFML).await;
    assert!(
        !svelte.join("src/routes/+layout.svelte").exists(),
        "no shell mapping must mean no layout emission"
    );
}

const WORKFLOW_IFML: &str = r#"
domain "sales" {
    schema "sales";
}

view "CustomerList" {
    label "Customers";

    component "grid" {
        type: list;
        data: Customer;
        fields: [name, status];
    }
}

view "CustomerDetail" {
    params { customerId: Uuid };

    component "info" {
        type: details;
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

#[tokio::test]
async fn workflow_view_renders_state_badges() {
    let dir = tempfile::tempdir().unwrap();
    let svelte = generate_svelte_with_domains(
        dir.path(),
        WORKFLOW_IFML,
        domains_toml_with_workflow(),
        None,
    )
    .await;

    let list = read(&svelte, "src/routes/customerlist/+page.svelte");
    assert!(
        list.contains(
            "<td><span class=\"workflow-state\" data-testid=\"grid-state\" data-workflow-state={item.status} data-workflow-terminal={['done'].includes(item.status as string) ? \"true\" : \"false\"}>{item.status}</span></td>"
        ),
        "list rows must carry a per-row state badge: {list}"
    );

    let details = read(&svelte, "src/routes/customerdetail/+page.svelte");
    assert!(
        details.contains(
            "<span class=\"workflow-state\" data-testid=\"info-state\" data-workflow-state={data.item?.workflow_state?.current_state} data-workflow-terminal={['done'].includes(data.item?.workflow_state?.current_state as string) ? \"true\" : \"false\"}>{data.item?.workflow_state?.current_state}</span>"
        ),
        "details badges read the merged workflow state: {details}"
    );

    let form = read(&svelte, "src/routes/customeredit/+page.svelte");
    assert!(
        form.contains(
            "<span class=\"workflow-state\" data-testid=\"editor-state\" data-workflow-state={editor_form_state.workflow_state?.current_state} data-workflow-terminal={['done'].includes(editor_form_state.workflow_state?.current_state as string) ? \"true\" : \"false\"}>{editor_form_state.workflow_state?.current_state}</span>"
        ),
        "form badges read the typed form state's merged workflow state: {form}"
    );
    assert!(
        form.contains(
            "const editor_form_state = $derived((data.formData ?? {}) as Record<string, any>);"
        ),
        "the form state const must be typed so property access typechecks: {form}"
    );
}

#[tokio::test]
async fn no_workflow_config_renders_no_badges() {
    let dir = tempfile::tempdir().unwrap();
    let svelte = generate_svelte(dir.path(), WORKFLOW_IFML).await;

    for route in [
        "src/routes/customerlist/+page.svelte",
        "src/routes/customerdetail/+page.svelte",
        "src/routes/customeredit/+page.svelte",
    ] {
        let page = read(&svelte, route);
        assert!(
            !page.contains("workflow-state") && !page.contains("data-workflow"),
            "no-workflow runs must stay badge-free: {route}: {page}"
        );
    }
}
