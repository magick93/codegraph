//! Integration tests for the IFML E2E test generator (`ifml-e2e-test`):
//! render tests, API-gated click-through / form tests, and the schema-less
//! gating rule (render tests only).

use std::collections::HashMap;
use std::path::Path;

use codegraph_config::{DomainConfig, IfmlComponentMappings};
use codegraph_core::mock::MockEngine;
use codegraph_core::traits::GraphIngestor;
use codegraph_core::types::{
    EdgeProperties, EdgeType, EventNode, ParameterDefinitionNode, PropertyNode, SchemaNode,
    ViewComponentNode, ViewContainerNode,
};
use codegraph_generate::ifml::e2e_test::IfmlE2eTestGenerator;
use codegraph_generate::traits::GlobalGenerator;
use codegraph_generate::ProjectConfig;

const EDITOR_FORM_SPEC: &str = r#"{"Form":{"fields":[{"name":"name","input":"Text","required":true,"validations":[],"values":[]}]}}"#;

fn test_config() -> DomainConfig {
    toml::from_str(
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
    .unwrap()
}

fn workflow_config() -> DomainConfig {
    toml::from_str(
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
"#,
    )
    .unwrap()
}

fn customer_schema() -> SchemaNode {
    SchemaNode {
        schema_id: "sales/customer_type.json".into(),
        title: "CustomerType".into(),
        description: None,
        schema_type: "object".into(),
        classification: "entity".into(),
        domain: Some("sales".into()),
        rel_path: "sales/customer_type.json".into(),
        pg_type: "JSONB".into(),
        rust_type: "customer::Model".into(),
        sea_orm_type: "customer".into(),
        rust_type_name: "CustomerType".into(),
        pg_table_name: "customer".into(),
        api_path_segment: "customer".into(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        custom_annotations: HashMap::new(),
    }
}

fn property(name: &str, rust_type: &str) -> PropertyNode {
    PropertyNode {
        name: name.to_string(),
        prop_type: "string".to_string(),
        description: None,
        format: None,
        is_required: true,
        is_nullable: false,
        is_array: false,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: name.to_string(),
        pg_column_type: "TEXT".to_string(),
        rust_field_name: name.to_string(),
        rust_field_type: rust_type.to_string(),
        sea_orm_type: "String".to_string(),
        render_strategy: "scalar".to_string(),
        ref_target: None,
        classification: None,
        projection: None,
        classification_kind: None,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
    }
}

async fn ingest_customer_schema(db: &MockEngine) {
    db.ingest_schema(&customer_schema()).await.unwrap();
    db.ingest_property(
        "CustomerType",
        "sales/customer_type.json",
        &property("name", "String"),
    )
    .await
    .unwrap();
    db.ingest_property(
        "CustomerType",
        "sales/customer_type.json",
        &property("age", "i32"),
    )
    .await
    .unwrap();
}

async fn ingest_view(db: &MockEngine, name: &str, label: Option<&str>) {
    ingest_view_with_flags(db, name, label, false).await;
}

async fn ingest_modal_view(db: &MockEngine, name: &str) {
    ingest_view_with_flags(db, name, None, true).await;
}

async fn ingest_view_with_flags(db: &MockEngine, name: &str, label: Option<&str>, is_modal: bool) {
    db.ingest_view_container(&ViewContainerNode {
        name: name.to_string(),
        label: label.map(str::to_string),
        is_xor: false,
        is_default: false,
        is_landmark: label.is_some(),
        is_modal,
        conditional_expression: None,
        domain: None,
        module_uses: None,
        roles: None,
    })
    .await
    .unwrap();
}

async fn ingest_component(
    db: &MockEngine,
    parent: &str,
    name: &str,
    component_type: &str,
    fields: &[&str],
    spec: Option<&str>,
) {
    db.ingest_view_component(&ViewComponentNode {
        name: name.to_string(),
        component_type: component_type.to_string(),
        mode: None,
        entity: Some("Customer".to_string()),
        fields: Some(fields.iter().map(|f| f.to_string()).collect()),
        filter: None,
        api_operation: None,
        spec: spec.map(str::to_string),
        conditional_expression: None,
        domain: None,
    })
    .await
    .unwrap();
    db.ingest_edge(
        &format!("vc:{parent}"),
        &format!("comp:{name}"),
        EdgeType::ContainsViewComponent,
        None,
    )
    .await
    .unwrap();
}

async fn ingest_event(db: &MockEngine, parent: &str, name: &str, event_type: &str) {
    db.ingest_event(&EventNode {
        name: name.to_string(),
        event_type: event_type.to_string(),
        params: None,
        conditional_expression: None,
        domain: None,
    })
    .await
    .unwrap();
    db.ingest_edge(
        &format!("comp:{parent}"),
        &format!("evt:{name}"),
        EdgeType::HasEvent,
        None,
    )
    .await
    .unwrap();
}

async fn ingest_navigation_flow(db: &MockEngine, event: &str, target: &str, binding: Option<&str>) {
    db.ingest_edge(
        &format!("evt:{event}"),
        &format!("vc:{target}"),
        EdgeType::NavigationFlow,
        binding
            .map(|b| EdgeProperties {
                target_param_binding: Some(b.to_string()),
                ..Default::default()
            })
            .as_ref(),
    )
    .await
    .unwrap();
}

async fn ingest_id_param(db: &MockEngine, view: &str, param: &str) {
    db.ingest_parameter_definition(&ParameterDefinitionNode {
        name: param.to_string(),
        direction: "in".to_string(),
        type_ref: "Uuid".to_string(),
        domain: None,
    })
    .await
    .unwrap();
    db.ingest_edge(
        &format!("vc:{view}"),
        &format!("param:{param}"),
        EdgeType::HasParameter,
        None,
    )
    .await
    .unwrap();
}

/// CustomerList --select--> CustomerDetail, plus a CustomerEdit form view
/// whose save event returns to the list.
async fn ingest_ifml_model(db: &MockEngine) {
    ingest_view(db, "CustomerList", Some("Customer Management")).await;
    ingest_view(db, "CustomerDetail", None).await;
    ingest_view(db, "CustomerEdit", None).await;

    ingest_component(db, "CustomerList", "grid", "list", &["name", "age"], None).await;
    ingest_component(db, "CustomerDetail", "info", "details", &["name"], None).await;
    ingest_component(
        db,
        "CustomerEdit",
        "editor",
        "form",
        &["name"],
        Some(EDITOR_FORM_SPEC),
    )
    .await;

    ingest_event(db, "grid", "comp_grid_select", "select").await;
    ingest_navigation_flow(
        db,
        "comp_grid_select",
        "CustomerDetail",
        Some(r#"{"customerId": "row.id"}"#),
    )
    .await;

    ingest_event(db, "editor", "comp_editor_save", "save").await;
    ingest_navigation_flow(db, "comp_editor_save", "CustomerList", None).await;

    ingest_id_param(db, "CustomerDetail", "customerId").await;
    ingest_id_param(db, "CustomerEdit", "customerId").await;
}

async fn generate(
    db: &MockEngine,
    output: &Path,
    mappings: Option<IfmlComponentMappings>,
) -> Vec<codegraph_generate::traits::GeneratedFile> {
    generate_with_config(db, output, mappings, &test_config()).await
}

async fn generate_with_config(
    db: &MockEngine,
    output: &Path,
    mappings: Option<IfmlComponentMappings>,
    config: &DomainConfig,
) -> Vec<codegraph_generate::traits::GeneratedFile> {
    let gen = IfmlE2eTestGenerator::new(output, "svelte").with_mappings(mappings);
    gen.generate(
        db,
        config,
        &[],
        &tera::Tera::default(),
        &ProjectConfig::default(),
    )
    .await
    .unwrap()
}

fn content_of(files: &[codegraph_generate::traits::GeneratedFile], suffix: &str) -> String {
    files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with(suffix))
        .unwrap_or_else(|| {
            panic!(
                "no file ending in {suffix}; got {:?}",
                files
                    .iter()
                    .map(|f| f.path.display().to_string())
                    .collect::<Vec<_>>()
            )
        })
        .content
        .clone()
}

#[tokio::test]
async fn emits_render_click_through_and_form_tests_with_schemas() {
    let engine = MockEngine::new();
    ingest_customer_schema(&engine).await;
    ingest_ifml_model(&engine).await;
    let dir = tempfile::tempdir().unwrap();

    let files = generate(&engine, dir.path(), None).await;

    let list_spec = content_of(&files, "tests/ifml/customer-list.spec.ts");
    assert!(
        list_spec.contains("test('renders Customer Management'"),
        "{list_spec}"
    );
    assert!(
        list_spec.contains("page.goto('/customerlist')"),
        "{list_spec}"
    );
    assert!(
        list_spec.contains("page.getByRole('heading', { name: 'Customer Management' })"),
        "{list_spec}"
    );
    assert!(
        list_spec.contains("getByTestId('grid-table')"),
        "{list_spec}"
    );

    assert!(
        list_spec.contains("request.post('/api/v1/sales/customer'"),
        "{list_spec}"
    );
    assert!(list_spec.contains("'name': 'Test name'"), "{list_spec}");
    assert!(list_spec.contains("'age': 42"), "{list_spec}");
    assert!(
        list_spec.contains("page.getByTestId('grid-row').first().click()"),
        "{list_spec}"
    );
    assert!(
        list_spec.contains("waitForURL(new RegExp('/customerdetail\\\\?customerId=[^&]+'))"),
        "{list_spec}"
    );

    let edit_spec = content_of(&files, "tests/ifml/customer-edit.spec.ts");
    assert!(
        edit_spec.contains("request.post('/api/v1/sales/customer'"),
        "{edit_spec}"
    );
    assert!(
        edit_spec.contains("page.goto(`/customeredit?customerId=${created.id}`)"),
        "{edit_spec}"
    );
    assert!(
        edit_spec.contains("page.getByTestId('editor-submit').click()"),
        "{edit_spec}"
    );
    assert!(
        edit_spec.contains("locator('[name=\"name\"]')).toBeInvalid()"),
        "{edit_spec}"
    );
    assert!(edit_spec.contains("fill('Updated name')"), "{edit_spec}");
    assert!(
        edit_spec.contains("waitForURL(new RegExp('/customerlist$'))"),
        "{edit_spec}"
    );
    assert!(
        edit_spec.contains("expect(persisted.name).toBe('Updated name')"),
        "{edit_spec}"
    );

    assert!(content_of(&files, "playwright.config.ts").contains("baseURL"));
    assert!(content_of(&files, "package.json").contains("@playwright/test"));
}

#[tokio::test]
async fn without_schemas_emits_render_tests_only() {
    let engine = MockEngine::new();
    ingest_ifml_model(&engine).await;
    let dir = tempfile::tempdir().unwrap();

    let files = generate(&engine, dir.path(), None).await;

    let list_spec = content_of(&files, "tests/ifml/customer-list.spec.ts");
    assert!(
        list_spec.contains("test('renders Customer Management'"),
        "{list_spec}"
    );
    assert!(
        list_spec.contains("getByTestId('grid-table')"),
        "{list_spec}"
    );
    assert!(
        !files.iter().any(|f| f.content.contains("request.post")),
        "no API-dependent tests may be emitted without schemas"
    );
    assert!(
        !files.iter().any(|f| f.content.contains("waitForURL")),
        "no navigation-flow tests may be emitted without schemas"
    );
    assert!(
        files
            .iter()
            .any(|f| f.path.to_string_lossy().ends_with("playwright.config.ts")),
        "render-only runs still scaffold the Playwright harness"
    );
}

#[tokio::test]
async fn mapped_components_use_mapping_testids() {
    let engine = MockEngine::new();
    ingest_customer_schema(&engine).await;
    ingest_ifml_model(&engine).await;
    let dir = tempfile::tempdir().unwrap();

    let mappings: IfmlComponentMappings = toml::from_str(
        r#"
[[component]]
type = "list"
path = "$lib/components/DataTable.svelte"
export = "DataTable"
testids = { root = "data-table", row = "data-row" }
"#,
    )
    .unwrap();

    let files = generate(&engine, dir.path(), Some(mappings)).await;
    let list_spec = content_of(&files, "tests/ifml/customer-list.spec.ts");
    assert!(
        list_spec.contains("getByTestId('data-table')"),
        "{list_spec}"
    );
    assert!(
        list_spec.contains("page.getByTestId('data-row').first().click()"),
        "{list_spec}"
    );
}

#[tokio::test]
async fn modal_click_through_asserts_dialog_and_close() {
    let engine = MockEngine::new();
    ingest_customer_schema(&engine).await;

    ingest_view(&engine, "CustomerList", Some("Customer Management")).await;
    ingest_modal_view(&engine, "CustomerDialog").await;
    ingest_component(
        &engine,
        "CustomerList",
        "grid",
        "list",
        &["name", "age"],
        None,
    )
    .await;
    ingest_component(
        &engine,
        "CustomerDialog",
        "editor",
        "form",
        &["name"],
        Some(EDITOR_FORM_SPEC),
    )
    .await;
    ingest_event(&engine, "grid", "comp_grid_select", "select").await;
    ingest_navigation_flow(
        &engine,
        "comp_grid_select",
        "CustomerDialog",
        Some(r#"{"customerId": "row.id"}"#),
    )
    .await;
    ingest_id_param(&engine, "CustomerDialog", "customerId").await;

    let mappings: IfmlComponentMappings = toml::from_str(
        r#"
[[component]]
role = "modal-view"
path = "$lib/components/Dialog.svelte"
export = "Dialog"
testids = { root = "customer-modal" }
"#,
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();

    let files = generate(&engine, dir.path(), Some(mappings)).await;
    let list_spec = content_of(&files, "tests/ifml/customer-list.spec.ts");
    assert!(
        list_spec
            .contains("waitForURL(new RegExp('/customerdialog\\\\?customerId=[^&]+&dialog=open'))"),
        "{list_spec}"
    );
    assert!(
        list_spec.contains("expect(page.getByTestId('customer-modal')).toBeVisible()"),
        "{list_spec}"
    );
    assert!(
        list_spec.contains("page.getByTestId('customerdialog-modal-close').click()"),
        "{list_spec}"
    );
    assert!(
        list_spec.contains("waitForURL(new RegExp('/customerlist$'))"),
        "{list_spec}"
    );
}

#[tokio::test]
async fn non_modal_click_through_keeps_plain_target_pattern() {
    let engine = MockEngine::new();
    ingest_customer_schema(&engine).await;
    ingest_ifml_model(&engine).await;

    let mappings: IfmlComponentMappings = toml::from_str(
        r#"
[[component]]
role = "action-control"
path = "$lib/components/Button.svelte"
"#,
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();

    let files = generate(&engine, dir.path(), Some(mappings)).await;
    let list_spec = content_of(&files, "tests/ifml/customer-list.spec.ts");
    assert!(
        list_spec.contains("waitForURL(new RegExp('/customerdetail\\\\?customerId=[^&]+'))"),
        "{list_spec}"
    );
    assert!(
        !list_spec.contains("dialog=open"),
        "non-modal targets must not gain the dialog param: {list_spec}"
    );
}

#[tokio::test]
async fn form_tests_use_mapped_button_testid() {
    let engine = MockEngine::new();
    ingest_customer_schema(&engine).await;
    ingest_ifml_model(&engine).await;

    let mappings: IfmlComponentMappings = toml::from_str(
        r#"
[[component]]
role = "action-control"
path = "$lib/components/Button.svelte"
export = "Button"
testids = { root = "ui-save-btn" }
"#,
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();

    let files = generate(&engine, dir.path(), Some(mappings)).await;
    let edit_spec = content_of(&files, "tests/ifml/customer-edit.spec.ts");
    assert!(
        edit_spec.contains("page.getByTestId('ui-save-btn').click()"),
        "{edit_spec}"
    );
    assert!(
        !edit_spec.contains("getByTestId('editor-submit')"),
        "mapped button testid must replace the fallback: {edit_spec}"
    );
}

#[tokio::test]
async fn scaffold_files_are_never_overwritten() {
    let engine = MockEngine::new();
    ingest_customer_schema(&engine).await;
    ingest_ifml_model(&engine).await;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("playwright.config.ts"), "// sentinel").unwrap();
    std::fs::write(dir.path().join("package.json"), "{}").unwrap();

    let files = generate(&engine, dir.path(), None).await;

    assert!(
        !files
            .iter()
            .any(|f| f.path.to_string_lossy().ends_with("playwright.config.ts")),
        "existing playwright.config.ts must not be regenerated"
    );
    assert!(
        !files
            .iter()
            .any(|f| f.path.to_string_lossy().ends_with("package.json")),
        "existing package.json must not be regenerated"
    );
    assert!(
        files
            .iter()
            .any(|f| f.path.to_string_lossy().ends_with(".spec.ts")),
        "specs are still emitted"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("playwright.config.ts")).unwrap(),
        "// sentinel"
    );
}

#[tokio::test]
async fn non_svelte_frameworks_emit_nothing() {
    let engine = MockEngine::new();
    ingest_customer_schema(&engine).await;
    ingest_ifml_model(&engine).await;
    let dir = tempfile::tempdir().unwrap();

    let gen = IfmlE2eTestGenerator::new(dir.path(), "react");
    let files = gen
        .generate(
            &engine,
            &test_config(),
            &[],
            &tera::Tera::default(),
            &ProjectConfig::default(),
        )
        .await
        .unwrap();
    assert!(files.is_empty());
}

#[tokio::test]
async fn render_tests_assert_shell_nav_when_mapped() {
    let engine = MockEngine::new();
    ingest_ifml_model(&engine).await;
    let dir = tempfile::tempdir().unwrap();

    let mappings: IfmlComponentMappings = toml::from_str(
        r#"
[[component]]
role = "shell"
path = "$lib/components/Nav.svelte"
export = "Nav"
testids = { root = "side-nav" }
"#,
    )
    .unwrap();
    let files = generate(&engine, dir.path(), Some(mappings)).await;
    let list_spec = content_of(&files, "tests/ifml/customer-list.spec.ts");
    assert!(
        list_spec.contains("await expect(page.getByTestId('side-nav')).toBeVisible();"),
        "{list_spec}"
    );
}

#[tokio::test]
async fn render_tests_omit_nav_assertion_without_shell_mapping() {
    let engine = MockEngine::new();
    ingest_ifml_model(&engine).await;
    let dir = tempfile::tempdir().unwrap();

    let files = generate(&engine, dir.path(), None).await;
    let list_spec = content_of(&files, "tests/ifml/customer-list.spec.ts");
    assert!(
        !list_spec.contains("side-nav"),
        "unmapped shell must not add nav assertions: {list_spec}"
    );
}

#[tokio::test]
async fn render_tests_assert_mapped_container_wrapper() {
    let engine = MockEngine::new();

    engine
        .ingest_view_container(&ViewContainerNode {
            name: "Checkout".to_string(),
            label: Some("Checkout".to_string()),
            is_xor: true,
            is_default: false,
            is_landmark: false,
            is_modal: false,
            conditional_expression: None,
            domain: None,
            module_uses: None,
            roles: None,
        })
        .await
        .unwrap();
    ingest_component(&engine, "Checkout", "editor", "form", &["name"], None).await;

    let dir = tempfile::tempdir().unwrap();
    let mappings: IfmlComponentMappings = toml::from_str(
        r#"
[[component]]
role = "presentation-container"
path = "$lib/components/Card.svelte"
export = "Card"
testids = { root = "card" }
"#,
    )
    .unwrap();
    let files = generate(&engine, dir.path(), Some(mappings)).await;
    let spec = content_of(&files, "tests/ifml/checkout.spec.ts");
    assert!(
        spec.contains("await expect(page.getByTestId('card')).toBeVisible();"),
        "{spec}"
    );
}

#[tokio::test]
async fn workflow_spec_emitted_for_schema_backed_workflow_entity() {
    let engine = MockEngine::new();
    ingest_customer_schema(&engine).await;
    ingest_ifml_model(&engine).await;
    let dir = tempfile::tempdir().unwrap();

    let files = generate_with_config(&engine, dir.path(), None, &workflow_config()).await;

    let list_spec = content_of(&files, "tests/ifml/customer-list.workflow.spec.ts");
    assert!(
        list_spec.contains("test.describe('Customer Management workflow'"),
        "{list_spec}"
    );
    assert!(
        list_spec.contains("shows the initial workflow state for grid"),
        "{list_spec}"
    );
    assert!(
        list_spec.contains("request.post('/api/v1/sales/customer'"),
        "{list_spec}"
    );
    assert!(list_spec.contains("'name': 'Test name'"), "{list_spec}");
    assert!(list_spec.contains("'age': 42"), "{list_spec}");
    assert!(
        list_spec.contains("page.goto('/customerlist')"),
        "{list_spec}"
    );
    assert!(
        list_spec.contains("page.getByTestId('grid-state')"),
        "{list_spec}"
    );
    assert!(
        list_spec.contains("toContainText('received')"),
        "{list_spec}"
    );

    let detail_spec = content_of(&files, "tests/ifml/customer-detail.workflow.spec.ts");
    assert!(
        detail_spec.contains("page.goto(`/customerdetail?customerId=${created.id}`)"),
        "{detail_spec}"
    );
    assert!(
        detail_spec.contains("page.getByTestId('info-state')"),
        "{detail_spec}"
    );

    let edit_spec = content_of(&files, "tests/ifml/customer-edit.workflow.spec.ts");
    assert!(
        edit_spec.contains("page.getByTestId('editor-state')"),
        "{edit_spec}"
    );
}

#[tokio::test]
async fn workflow_spec_not_emitted_without_workflow_config() {
    let engine = MockEngine::new();
    ingest_customer_schema(&engine).await;
    ingest_ifml_model(&engine).await;
    let dir = tempfile::tempdir().unwrap();

    let files = generate(&engine, dir.path(), None).await;

    assert!(
        !files
            .iter()
            .any(|f| f.path.to_string_lossy().ends_with(".workflow.spec.ts")),
        "no workflow config must mean no workflow specs"
    );
    assert!(
        files
            .iter()
            .any(|f| f.path.to_string_lossy().ends_with("customer-list.spec.ts")),
        "regular specs are still emitted"
    );
}

#[tokio::test]
async fn workflow_spec_requires_schema_backing() {
    let engine = MockEngine::new();
    ingest_ifml_model(&engine).await;
    let dir = tempfile::tempdir().unwrap();

    let files = generate_with_config(&engine, dir.path(), None, &workflow_config()).await;

    assert!(
        !files
            .iter()
            .any(|f| f.path.to_string_lossy().ends_with(".workflow.spec.ts")),
        "schema-less runs must not emit workflow specs"
    );
}
