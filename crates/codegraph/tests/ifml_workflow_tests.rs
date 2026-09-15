//! Workflow UI v2 RED tests (issue #198 + #208 slice): the generated
//! `{view}.workflow.spec.ts` must gain a transition round trip (create
//! fixture in the initial state → open the details view → click the enabled
//! transition button → badge shows the new state → API GET confirms
//! persistence), and workflow specs must no longer skip mapped components
//! (badges render as siblings next to mapped invocations).
//!
//! These pin SPEC CONTENT only (no Playwright run) — the full-stack
//! acceptance lives in `ifml_codegen_gate.rs`.

use std::path::Path;

const WORKFLOW_APP_IFML: &str = r#"
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
    label "Customer";

    component "info" {
        type: details;
        data: Customer;
        fields: [name, status];
    }
}
"#;

const CUSTOMER_SCHEMA: &str = r#"{
  "$id": "CustomerType.json",
  "title": "CustomerType",
  "description": "A customer",
  "type": "object",
  "properties": {
    "id": { "type": "string", "format": "uuid", "description": "Unique identifier" },
    "name": { "type": "string", "description": "Customer name" },
    "status": { "type": "string", "enum": ["draft", "submitted", "approved", "rejected"], "description": "Workflow status" }
  }
}"#;

fn domains_toml_with_transitions() -> String {
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
initial_state = "draft"
states = ["draft", "submitted", "approved", "rejected"]
terminal_states = ["approved", "rejected"]
generate_action_endpoints = true

[domains.sales.entity_config.CustomerType.workflow.transitions]
draft = ["submitted"]
submitted = ["approved", "rejected"]
"#
    .to_string()
}

async fn generate(dir: &Path, ifml: &str, mappings: Option<&Path>) -> std::path::PathBuf {
    let ifml_path = dir.join("app.ifml");
    std::fs::write(&ifml_path, ifml).unwrap();
    let domains_toml_path = dir.join("domains.toml");
    std::fs::write(&domains_toml_path, domains_toml_with_transitions()).unwrap();
    let classifier_path = dir.join("classifier.toml");
    std::fs::write(&classifier_path, "# minimal classifier config\n").unwrap();
    let schema_file = dir.join("schemas").join("sales").join("CustomerType.json");
    std::fs::create_dir_all(schema_file.parent().unwrap()).unwrap();
    std::fs::write(&schema_file, CUSTOMER_SCHEMA).unwrap();
    let schemas = dir.join("schemas");
    let output = dir.join("out");

    codegraph::driver::ifml_generate(codegraph::driver::IfmlGenerateArgs {
        config_path: &domains_toml_path,
        output: &output,
        ifml_files: &[ifml_path],
        schemas: Some(&schemas),
        classifier: Some(&classifier_path),
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

/// The workflow spec for a details view must carry a transition round trip:
/// the initial fixture (draft state) opens the view, clicks the ENABLED
/// transition button (`{component}-transition-{target}` testid), the state
/// badge shows the target state, and an API GET confirms persistence.
#[tokio::test]
async fn workflow_spec_gains_transition_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let svelte = generate(dir.path(), WORKFLOW_APP_IFML, None).await;

    let spec = std::fs::read_to_string(
        svelte
            .join("tests/ifml")
            .join("customer-detail.workflow.spec.ts"),
    )
    .expect("workflow spec must be emitted for the details view");

    assert!(
        spec.contains("test('transitions info from draft to submitted'"),
        "the spec must gain a transition test from the initial state to the first valid target:\n{spec}"
    );
    assert!(
        spec.contains("getByTestId('info-transition-submitted')"),
        "the test clicks the enabled transition button (draft → submitted):\n{spec}"
    );
    assert!(
        spec.contains("getByTestId('info-state')") && spec.contains("toContainText('submitted')"),
        "the state badge must show the new state after the click:\n{spec}"
    );
    assert!(
        spec.contains(".status).toBe('submitted')"),
        "an API GET must confirm the transition persisted:\n{spec}"
    );

    assert!(
        spec.contains("test('shows the initial workflow state for info'"),
        "the initial-state assertion stays in place:\n{spec}"
    );
}

/// Workflow specs must not skip mapped components: the badge renders as a
/// sibling next to the mapped invocation, so the initial-state + transition
/// assertions work there too.
#[tokio::test]
async fn workflow_spec_emitted_for_mapped_components() {
    let dir = tempfile::tempdir().unwrap();
    let mappings = dir.path().join("ifml-components.toml");
    std::fs::write(
        &mappings,
        r#"
[[component]]
kind = "details"
path = "$lib/components/DetailCard.svelte"
export = "DetailCard"
testids = { root = "detail-card" }
"#,
    )
    .unwrap();
    let svelte = generate(dir.path(), WORKFLOW_APP_IFML, Some(&mappings)).await;

    let spec = std::fs::read_to_string(
        svelte
            .join("tests/ifml")
            .join("customer-detail.workflow.spec.ts"),
    )
    .expect("workflow spec must be emitted even when the component is mapped");

    assert!(
        spec.contains("test('shows the initial workflow state for info'"),
        "the mapped component's initial-state assertion is un-skipped:\n{spec}"
    );
    assert!(
        spec.contains("getByTestId('info-transition-submitted')"),
        "the mapped component's transition round trip is un-skipped:\n{spec}"
    );
}
