//! Compile pin for the injected-scope `when` plane (issue #663, the #551
//! policy): the generated app over the guard fixture must `cargo check`
//! clean — the async authorize fn (`&AppState` + `&ApiKeyInfo` params, the
//! own-record FK extraction, the sibling query slot's `find_by_id` load,
//! and the scoped comparison) compiles against the generated app exactly
//! like the synchronous own-record gate does.
//!
//! Its FIRST run caught the design-op override missing rust-name-keyed
//! lookups (the `TimecardType` design declared no `delete`, yet the CRUD
//! DELETE handler called the design-gated command plane — E0599; fixed in
//! `resolve_entity_operations` + `operations_for_rust_name`).
//!
//! OWN TEST BINARY on purpose: the process-global `TYPE_REGISTRY`
//! (type_registry.rs) is shared by every `driver::run` in one process,
//! and this fixture declares class titles (`PayRunType`) that the payroll
//! compile-gate legs also declare in a different domain — a shared
//! process would resolve the first registrant's module paths.
//! TODO(#746): per-run type registry.
//!
//! Path deps (`dependency_strategy = "path"`) keep the gate network-free.
//!
//! ```text
//! cargo test -p codegraph --test service_ops_injected_guard_gate -- --ignored --nocapture
//! ```

#[path = "service_ops_fixture.rs"]
mod fixture;

use std::fs;
use std::path::Path;

use fixture::{gate_tempdir, ingest_guard, write_guard_fixture};

/// The fixture profiles: the workspace default with path deps (the gate
/// must compile without network) and the gRPC generators stripped (no
/// protoc requirement) — the same recipe `service_ops_compile_gate`
/// uses (`type_contracts_base` pinned at the local checkout, the WS1
/// finding).
fn write_profiles(dir: &Path) -> std::path::PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    let type_contracts = workspace_root.join("crates/codegraph-type-contracts");
    let workspace_profiles = workspace_root.join("profiles.toml");
    let mut text = fs::read_to_string(&workspace_profiles).unwrap();
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
async fn generated_app_with_injected_guard_compiles() {
    let fixture = write_guard_fixture();
    let _engine = ingest_guard(&fixture).await;

    let out_dir = gate_tempdir();
    let output = out_dir.path().join("app");
    fs::create_dir_all(&output).unwrap();
    let fixture_root = fixture.config.parent().unwrap().to_path_buf();
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
    .expect("generation over the guard fixture must succeed");

    // The async gate must be ON DISK before the build spend: the sibling
    // query slot spelling and the FK field are compile-coupled (a misspelt
    // slot or DTO field is an E0560, not a silent seam).
    let handler = fs::read_to_string(output.join("src/api/guardpay/pay_run_handler.rs"))
        .expect("the guarded handler must emit");
    assert!(
        handler.contains("async fn service_op_submit_authorize"),
        "the injected guard must render the async authorize fn:\n{handler}"
    );
    assert!(
        handler.contains("state.guardpay_timecard_queries"),
        "the load must ride the injected entity's query slot:\n{handler}"
    );
    assert!(
        handler.contains("response.timecard_id"),
        "the id must come from the own-record FK:\n{handler}"
    );

    codegraph::driver::check_generated_output(&output)
        .expect("the generated app with an injected-scope guard must compile");
}
