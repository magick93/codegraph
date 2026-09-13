//! Integration tests for the decoupled IFML-only UI generation path
//! (`codegraph::driver::ifml_generate`). Each run uses a fresh in-memory
//! backend (the driver creates one per call), so reruns simulate a clean
//! graph without cross-run state.

use std::path::Path;

/// Two-view IFML fixture (CustomerList → CustomerDetail navigation).
const APP_IFML: &str = r#"
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

/// Same fixture with CustomerDetail removed (incremental regeneration).
const APP_IFML_NO_DETAIL: &str = r#"
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
    }
}
"#;

/// Minimal JSON schema for the entity resolution enrichment path.
const CUSTOMER_SCHEMA: &str = r#"{
  "$id": "CustomerType.json",
  "title": "CustomerType",
  "description": "A customer",
  "type": "object",
  "properties": {
    "id": { "type": "string", "format": "uuid", "description": "Unique identifier" },
    "name": { "type": "string", "description": "Customer name" },
    "email": { "type": "string", "format": "email", "description": "Email address" },
    "phone": { "type": "string", "description": "Phone number" },
    "status": { "type": "string", "enum": ["active", "inactive"], "description": "Customer status" }
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

fn collect_sql_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if !dir.exists() {
        return out;
    }
    for entry in walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.path().extension().is_some_and(|e| e == "sql") {
            out.push(entry.path().to_path_buf());
        }
    }
    out
}

fn frameworks(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| s.to_string()).collect()
}

fn make_args<'a>(
    domains_toml: &'a Path,
    output: &'a Path,
    ifml_files: &'a [std::path::PathBuf],
    frameworks: &'a [String],
) -> codegraph::driver::IfmlGenerateArgs<'a> {
    codegraph::driver::IfmlGenerateArgs {
        config_path: domains_toml,
        output,
        ifml_files,
        schemas: None,
        classifier: None,
        frameworks,
        profiles_config_path: None,
        template_dir: &[],
        ifml_components: None,
    }
}

#[tokio::test]
async fn ifml_generate_svelte_produces_routes_only() {
    let dir = tempfile::tempdir().unwrap();
    write_domains_toml(dir.path());
    let ifml_path = dir.path().join("app.ifml");
    std::fs::write(&ifml_path, APP_IFML).unwrap();
    let output = dir.path().join("out");
    let domains_toml = dir.path().join("domains.toml");

    codegraph::driver::ifml_generate(make_args(
        &domains_toml,
        &output,
        &[ifml_path],
        &frameworks(&["svelte"]),
    ))
    .await
    .unwrap();

    assert!(output
        .join("svelte/src/routes/customerlist/+page.svelte")
        .exists());
    assert!(output
        .join("svelte/src/routes/customerdetail/+page.svelte")
        .exists());
    assert!(output.join("svelte/src/lib/routes.ts").exists());

    // IFML E2E tests + Playwright harness scaffold.
    let list_spec = std::fs::read_to_string(
        output
            .join("svelte/tests/ifml")
            .join("customer-list.spec.ts"),
    )
    .unwrap();
    assert!(list_spec.contains("test('renders Customer Management'"),);
    assert!(output.join("svelte/playwright.config.ts").exists());
    assert!(output.join("svelte/package.json").exists());

    // Narrowness: nothing but the IFML framework output is generated.
    assert!(!output.join("migrations").exists());
    assert!(!output.join("src/domain").exists());
    assert!(!output.join("src/api").exists());
    assert!(!output.join("ui").exists());
    let sql_files = collect_sql_files(&output);
    assert!(sql_files.is_empty(), "unexpected .sql files: {sql_files:?}");
}

#[tokio::test]
async fn ifml_generate_without_schemas_emits_only_render_tests() {
    let dir = tempfile::tempdir().unwrap();
    write_domains_toml(dir.path());
    let ifml_path = dir.path().join("app.ifml");
    std::fs::write(&ifml_path, APP_IFML).unwrap();
    let output = dir.path().join("out");
    let domains_toml = dir.path().join("domains.toml");

    codegraph::driver::ifml_generate(make_args(
        &domains_toml,
        &output,
        &[ifml_path],
        &frameworks(&["svelte"]),
    ))
    .await
    .unwrap();

    let tests_dir = output.join("svelte/tests/ifml");
    let mut spec_contents = Vec::new();
    for entry in std::fs::read_dir(&tests_dir).unwrap().flatten() {
        if entry.path().extension().is_some_and(|e| e == "ts") {
            spec_contents.push(std::fs::read_to_string(entry.path()).unwrap());
        }
    }
    assert!(
        !spec_contents.is_empty(),
        "render specs should be emitted for views with testable behavior"
    );
    let list_spec = std::fs::read_to_string(tests_dir.join("customer-list.spec.ts")).unwrap();
    assert!(list_spec.contains("test('renders Customer Management'"));
    assert!(
        !spec_contents
            .iter()
            .any(|c| c.contains("request.post") || c.contains("waitForURL")),
        "schema-less runs must emit render tests only"
    );
}

#[tokio::test]
async fn ifml_generate_with_schemas_enriches_entity_resolution() {
    let dir = tempfile::tempdir().unwrap();
    write_domains_toml(dir.path());
    let classifier_path = dir.path().join("classifier.toml");
    std::fs::write(&classifier_path, "# minimal classifier config\n").unwrap();
    let schemas_dir = dir.path().join("schemas");
    let schema_file = schemas_dir
        .join("sales")
        .join("json")
        .join("CustomerType.json");
    std::fs::create_dir_all(schema_file.parent().unwrap()).unwrap();
    std::fs::write(&schema_file, CUSTOMER_SCHEMA).unwrap();
    let ifml_path = dir.path().join("app.ifml");
    std::fs::write(&ifml_path, APP_IFML).unwrap();
    let output = dir.path().join("out");
    let domains_toml = dir.path().join("domains.toml");

    codegraph::driver::ifml_generate(codegraph::driver::IfmlGenerateArgs {
        config_path: &domains_toml,
        output: &output,
        ifml_files: &[ifml_path],
        schemas: Some(&schemas_dir),
        classifier: Some(&classifier_path),
        frameworks: &frameworks(&["svelte"]),
        profiles_config_path: None,
        template_dir: &[],
        ifml_components: None,
    })
    .await
    .unwrap();

    assert!(output
        .join("svelte/src/routes/customerlist/+page.svelte")
        .exists());
    assert!(output
        .join("svelte/src/routes/customerdetail/+page.svelte")
        .exists());
}

#[tokio::test]
async fn ifml_generate_removes_stale_routes_incrementally() {
    let dir = tempfile::tempdir().unwrap();
    write_domains_toml(dir.path());
    let ifml_path = dir.path().join("app.ifml");
    std::fs::write(&ifml_path, APP_IFML).unwrap();
    let output = dir.path().join("out");
    let domains_toml = dir.path().join("domains.toml");

    codegraph::driver::ifml_generate(make_args(
        &domains_toml,
        &output,
        std::slice::from_ref(&ifml_path),
        &frameworks(&["svelte"]),
    ))
    .await
    .unwrap();
    assert!(output
        .join("svelte/src/routes/customerdetail/+page.svelte")
        .exists());

    std::fs::write(&ifml_path, APP_IFML_NO_DETAIL).unwrap();
    codegraph::driver::ifml_generate(make_args(
        &domains_toml,
        &output,
        &[ifml_path],
        &frameworks(&["svelte"]),
    ))
    .await
    .unwrap();

    assert!(
        !output.join("svelte/src/routes/customerdetail").exists(),
        "stale CustomerDetail route should be removed"
    );
    assert!(output
        .join("svelte/src/routes/customerlist/+page.svelte")
        .exists());
}

#[tokio::test]
async fn ifml_generate_requires_ifml_files() {
    let dir = tempfile::tempdir().unwrap();
    write_domains_toml(dir.path());
    let output = dir.path().join("out");
    let domains_toml = dir.path().join("domains.toml");

    let err = codegraph::driver::ifml_generate(make_args(
        &domains_toml,
        &output,
        &[],
        &frameworks(&["svelte"]),
    ))
    .await
    .unwrap_err();

    assert!(
        err.to_string().contains("--ifml-files"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn ifml_generate_multiple_frameworks() {
    let dir = tempfile::tempdir().unwrap();
    write_domains_toml(dir.path());
    let ifml_path = dir.path().join("app.ifml");
    std::fs::write(&ifml_path, APP_IFML).unwrap();
    let output = dir.path().join("out");
    let domains_toml = dir.path().join("domains.toml");

    codegraph::driver::ifml_generate(codegraph::driver::IfmlGenerateArgs {
        config_path: &domains_toml,
        output: &output,
        ifml_files: &[ifml_path],
        schemas: None,
        classifier: None,
        frameworks: &frameworks(&["svelte", "react"]),
        profiles_config_path: None,
        template_dir: &[],
        ifml_components: None,
    })
    .await
    .unwrap();

    assert!(output
        .join("svelte/src/routes/customerlist/+page.svelte")
        .exists());
    assert!(output.join("react/app/customer-list/page.tsx").exists());
    assert!(output.join("react/app/customer-detail/page.tsx").exists());
}
