//! The IFML expression checks (E0100–E0103) run against the `.mox` domain
//! union (issue #636).
//!
//! Before #636 the front door (`ifml_compile::compile_and_check_ifml`)
//! passed `domains: None`, so a guard/`filter:`/binding expression over
//! an unknown field or a type-invalid operator compiled clean. The union is
//! now the `.ifml` file's gathered `.mox` closure, compiled the way the
//! pherolang CLI compiles it, and fed to BOTH the checker and the manifest.

use std::fs;
use std::path::PathBuf;

use codegraph::ifml_compile::compile_and_check_ifml;
use tempfile::TempDir;

const SHOP_MOX: &str = "package shop\n\nclass Product {\n    String title\n    long quantity\n}\n";

fn ifml(filter: &str) -> String {
    format!(
        r#"import "shop.mox";

view "ProductList" {{
    component "grid" {{
        type: list;
        data: Product;
        fields: [title, quantity];
        filter: {filter};
    }}
}}
"#
    )
}

fn write(dir: &TempDir, filter: &str) -> PathBuf {
    fs::write(dir.path().join("shop.mox"), SHOP_MOX).unwrap();
    let path = dir.path().join("app.ifml");
    fs::write(&path, ifml(filter)).unwrap();
    path
}

#[test]
fn a_valid_expression_compiles_against_the_union() {
    let dir = TempDir::new().unwrap();
    let compiled = compile_and_check_ifml(&write(&dir, "quantity > 0"))
        .unwrap_or_else(|e| panic!("a valid filter must compile: {e}"));
    assert!(!compiled.manifest.views.is_empty());
}

#[test]
fn an_unknown_field_in_a_filter_fails_naming_the_token() {
    let dir = TempDir::new().unwrap();
    let err = compile_and_check_ifml(&write(&dir, "nonexistent > 0"))
        .expect_err("an unknown field must fail the expression check")
        .to_string();
    assert!(err.contains("nonexistent"), "{err}");
    assert!(
        err.contains("E010"),
        "an E0100–E0103 code must be named: {err}"
    );
}

/// A `.mox` import that does not compile fails the union BY NAME — never a
/// silent fall-back to the unchecked `domains: None`.
#[test]
fn a_union_that_cannot_be_built_is_a_named_error() {
    let dir = TempDir::new().unwrap();
    let path = write(&dir, "quantity > 0");
    fs::write(
        dir.path().join("shop.mox"),
        "package shop\n\nclass Product {\n    Nope title\n}\n",
    )
    .unwrap();
    let err = compile_and_check_ifml(&path)
        .expect_err("a broken domain must fail the front door")
        .to_string();
    assert!(err.contains("domain union"), "{err}");
    assert!(err.contains("shop.mox"), "{err}");
}

/// Doctor runs the same front door, so it inherits the expression checks.
#[test]
fn doctor_fails_on_a_bad_expression() {
    use codegraph::init::commands::{DoctorArgs, cmd_doctor};
    let dir = TempDir::new().unwrap();
    let path = write(&dir, "nonexistent > 0");
    fs::write(
        dir.path().join("domains.toml"),
        "[domains.shop]\nlabel = \"Shop\"\nschema_dir = \"shop\"\npostgres_schema = \"shop\"\n",
    )
    .unwrap();
    let summary = cmd_doctor(&DoctorArgs {
        config: dir.path().join("domains.toml"),
        schemas: None,
        classifier: None,
        profiles_config: None,
        mox_files: vec![dir.path().join("shop.mox")],
        rosetta_files: vec![],
        ddd_files: vec![],
        evt_files: vec![],
        deploy_files: vec![],
        ifml_files: vec![path],
        extension_points: None,
        fail_on_findings: false,
        graph_artifact: None,
        emit_graph: None,
        graph_reachability: vec![],
    });
    assert!(
        summary.map_or(true, |s| s.hard_failures > 0),
        "doctor must fail the bad expression"
    );
}

/// The run path fails the bad expression too (driver Pass 1b is the same
/// front door).
#[tokio::test]
async fn the_run_path_fails_on_a_bad_expression() {
    let dir = TempDir::new().unwrap();
    let path = write(&dir, "nonexistent > 0");
    fs::write(
        dir.path().join("domains.toml"),
        "[domains.shop]\nlabel = \"Shop\"\nschema_dir = \"shop\"\npostgres_schema = \"shop\"\n",
    )
    .unwrap();
    let mox = vec![dir.path().join("shop.mox")];
    let ifml_files = vec![path];
    let err = codegraph::driver::run(codegraph::driver::RunArgs {
        integration_manifests: &[],
        schemas: None,
        classifier: None,
        config_path: &dir.path().join("domains.toml"),
        output: &dir.path().join("out"),
        extension_points_path: None,
        profile_name: "default",
        variant: None,
        profiles_config_path: None,
        no_post_gen: true,
        template_dir: &[],
        ifml_files: &ifml_files,
        openapi_files: &[],
        mox_files: &mox,
        rosetta_files: &[],
        ddd_files: &[],
        evt_files: &[],
        deploy_files: &[],
        ifml_framework: &[],
        ifml_components: None,
        ifml_design_system: None,
        codegraph_rev: None,
        check: false,
        ux_rules: None,
    })
    .await
    .expect_err("the run must fail the bad expression")
    .to_string();
    assert!(err.contains("nonexistent"), "{err}");
}
