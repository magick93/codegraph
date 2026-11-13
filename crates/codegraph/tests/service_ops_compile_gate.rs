//! Compile pin for the service-ops plane (issue #590, the #551 policy):
//! the generated app over a fixture with service ops must `cargo check`
//! clean — routes, authorize fns, transition guards, delegation legs, and
//! the 501 seams compile together with the rest of the app.
//!
//! The fixture carries the depth-2 GRANDCHILD surface too (issue #629 — the
//! WS4 v1 deferral): nested action routes under both ancestors, three-uuid
//! handler bindings, and the scoped loads compile with everything else.
//! Before this extension the depth-2 branch rendered routes that compiled
//! NOWHERE — exactly the #551 silent-emission class the pin policy exists
//! for.
//!
//! Path deps (`dependency_strategy = "path"`, issue #347) keep the gate
//! network-free. Skips gracefully when `cargo` is absent.
//!
//! ```text
//! cargo test -p codegraph --test service_ops_compile_gate -- --ignored --nocapture
//! ```

#[path = "service_ops_fixture.rs"]
mod fixture;

use std::fs;
use std::path::Path;

use fixture::{
    DOMAINS_TOML, PAY_RUN_MOX, PAY_RUN_SCHEMA, PAYROLL_DDD, ingest, tempdir, write_fixture_in,
};

/// The fixture profiles: the workspace default with path deps (the gate
/// must compile without network) and the gRPC generators stripped (no
/// protoc requirement). Also pins `type_contracts_base` at the local
/// checkout: without it the generated domain-types crate pins
/// `codegraph-type-contracts` at this checkout's (unpushed) git rev and
/// the gate's cargo check fetch fails (the WS1 finding).
fn write_profiles(dir: &Path) -> std::path::PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    let type_contracts = workspace_root.join("crates/codegraph-type-contracts");
    let workspace_profiles = workspace_root.join("profiles.toml");
    let mut text = fs::read_to_string(&workspace_profiles).unwrap();
    // The init-scaffold meta shape: the domain-types crate lands at
    // crates/domain-types inside the output (the root stays the app
    // manifest), and path deps keep the gate network-free (#347).
    text = text.replace(
        "[profiles.default.meta]\n",
        &format!(
            "[profiles.default.meta]\n\
             domain_types_base = \"crates/domain-types\"\n\
             type_contracts_base = \"{}\"\n",
            type_contracts.display()
        ),
    );
    text = text.replace(
        "[profiles.default.features]\n",
        "[profiles.default.features]\ndependency_strategy = \"path\"\n",
    );
    // Strip grpc generator lines (protoc is not a gate prerequisite here).
    let filtered: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim_start().starts_with("\"grpc_"))
        .collect();
    let path = dir.join("profiles.toml");
    fs::write(&path, filtered.join("\n")).unwrap();
    path
}

#[tokio::test]
#[ignore = "slow: cargo check over the generated app; needs the full dep stack"]
async fn generated_app_with_service_ops_compiles() {
    let fixture = write_fixture_in("service_ops_gate", PAYROLL_DDD);
    let _engine = ingest(&fixture).await;

    let out_dir = tempdir();
    let output = out_dir.path().join("app");
    fs::create_dir_all(&output).unwrap();

    // The fixture's domains.toml + a path-dep profiles.toml beside it.
    let fixture_root = fixture.config.parent().unwrap().to_path_buf();
    let _ = DOMAINS_TOML;
    let _ = (PAY_RUN_SCHEMA, PAY_RUN_MOX);
    let profiles_path = write_profiles(&fixture_root);

    codegraph::driver::run(codegraph::driver::RunArgs {
        schemas: None,
        classifier: None,
        config_path: &fixture.config,
        output: &output,
        extension_points_path: None,
        integration_manifests: &[],
        profile_name: "default",
        variant: None,
        profiles_config_path: Some(profiles_path),
        no_post_gen: true,
        template_dir: &[],
        ifml_files: &[],
        openapi_files: &[],
        mox_files: std::slice::from_ref(&fixture.mox),
        rosetta_files: &[],
        ddd_files: std::slice::from_ref(&fixture.ddd),
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
    .expect("generation over the service-ops fixture must succeed");

    // The plane must be present in the generated app before any build spend.
    let handler = fs::read_to_string(output.join("src/api/payroll/pay_run_handler.rs")).unwrap();
    assert!(
        handler.contains("service_op_approve"),
        "the op handlers must emit"
    );
    assert!(
        handler.contains("service_op_approve_authorize"),
        "the authorize fns must emit"
    );

    // The depth-2 grandchild surface (issue #629 — the #551 gap) must be
    // present too: routes fn, nested handlers, authorize fns. The pins
    // before any build spend keep a missing depth-2 emission from burning a
    // cargo check to discover.
    let grandchild_handler =
        fs::read_to_string(output.join("src/api/payroll/pay_run_line_adjustment_handler.rs"))
            .expect("the grandchild handler must emit (issue #629)");
    assert!(
        grandchild_handler.contains("service_op_recalculate"),
        "the grandchild op handlers must emit"
    );
    assert!(
        grandchild_handler.contains("service_op_recalculate_authorize"),
        "the grandchild authorize fns must emit"
    );
    let router = fs::read_to_string(output.join("src/api/payroll/router.rs")).unwrap();
    assert!(
        router.contains(
            ".nest(\"/{pay_run_line_id}/pay-run-line-adjustment\", pay_run_line_adjustment_routes())"
        ),
        "the child router must nest the grandchild routes fn"
    );

    codegraph::driver::check_generated_output(&output)
        .expect("the generated app with service ops must pass the compile gate");
}
