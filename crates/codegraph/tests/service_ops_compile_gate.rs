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
//! Issue #662 extends the same fixture with a checked `.ddd` statement body
//! (`applyPayRun`): the lowered `let`/`guard`/`assign`/`self.save(..)` body
//! really compiles against the generated command/query planes — the pin's
//! teeth are that the body's Update-DTO name, parameter types, and
//! address-scoped reload are spelled exactly as the rest of the app spells
//! them.
//!
//! Path deps (`dependency_strategy = "path"`, issue #347) keep the gate
//! network-free. Skips gracefully when `cargo` is absent.
//!
//! ```text
//! cargo test -p codegraph --test service_ops_compile_gate -- --ignored --nocapture
//! ```
//!
//! The injected-scope `when` leg (issue #663) lives in its OWN test binary
//! (`service_ops_injected_guard_gate.rs`): the process-global
//! `TYPE_REGISTRY` (type_registry.rs) is shared by every `driver::run` in
//! one process, and this file's legs run the PAYROLL fixture — a second,
//! different-domain fixture declaring the same class titles
//! (`PayRunType`) would resolve the first registrant's module paths.
//! TODO(#746): per-run type registry (the process-global is a concurrency
//! hazard for any two parallel generations).

#[path = "service_ops_fixture.rs"]
mod fixture;

use std::fs;
use std::path::Path;

use fixture::{
    DOMAINS_TOML, PAY_RUN_MOX, PAY_RUN_SCHEMA, PAYROLL_DDD, gate_tempdir, ingest, write_fixture_in,
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

    let out_dir = gate_tempdir();
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

    // The lowered statement body (issue #662) must be ON DISK before the
    // build spend, and it must reach the same command/query planes the rest
    // of the app does (a misspelt Update DTO or read arity is a compile
    // error, not a silent seam).
    assert!(
        handler.contains("statement body for `applyPayRun`")
            && handler.contains("_commands.update(")
            && handler.contains("dto_update::UpdatePayRunRequest"),
        "the statement body must lower into the handler before the build:\n{handler}"
    );

    codegraph::driver::check_generated_output(&output)
        .expect("the generated app with service ops must pass the compile gate");
}

/// Issue #660: the AUTHORED-BINDING plane compiles. The same app recipe over
/// the binding fixture (two machine-bearing designs in one module, every
/// machine-bound op declaring `transition <EnumType>.<Trigger>`) — the
/// emitted transition arms must compile against the generated Update DTOs
/// exactly like the heuristic ones, and the qualifier resolution must not
/// leave a dangling arm.
#[tokio::test]
#[ignore = "slow: cargo check over the generated app; needs the full dep stack"]
async fn generated_app_with_authored_transition_bindings_compiles() {
    let fixture = fixture::write_binding_fixture();
    let _engine = fixture::ingest_binding(&fixture).await;

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
    .expect("generation over the binding fixture must succeed");

    for (entity, arm) in [
        ("release", r#""Held" => Some("SignedOff")"#),
        ("review", r#""Pending" => Some("Cleared")"#),
    ] {
        let handler =
            fs::read_to_string(output.join(format!("src/api/release/{entity}_handler.rs")))
                .unwrap();
        assert!(
            handler.contains(arm),
            "the authored transition arm must lower into the handler before the build: {arm}\n{handler}"
        );
    }

    codegraph::driver::check_generated_output(&output)
        .expect("the generated app with authored transition bindings must compile");
}

/// Issue #661: the AGGREGATE CHILD OPERATION plane compiles. The fixture
/// models `children PayRunType { … => lines.add|save|remove }` over a
/// repository-bearing child entity declared as a nested child, so the
/// emitted handlers drive the CHILD's create/update/delete command planes
/// through the parent's aggregate path — the collection-level `add` (create
/// + reload), the item-level `save` (update + reload), and `remove`
/// (delete). The pin's teeth: the child action handlers must name the
/// child's real DTO modules and command methods (a misspelt
/// `Create{Child}Request` or a create signature without `parent_id` is a
/// compile error, never a silent seam).
#[tokio::test]
#[ignore = "slow: cargo check over the generated app; needs the full dep stack"]
async fn generated_app_with_aggregate_child_operations_compiles() {
    let fixture = fixture::write_child_ops_fixture();
    let engine = fixture::ingest_child_ops(&fixture).await;
    let _ = &engine;

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
    .expect("generation over the aggregate-child fixture must succeed");

    // The child action surface must be ON DISK before the build spend, and
    // it must reach the child's OWN planes (the #551 lesson: a misspelt
    // module is a compile error only if we bother to compile it).
    let handler =
        fs::read_to_string(output.join("src/api/payroll/pay_run_line_handler.rs")).unwrap();
    for needle in [
        "service_op_add_line",
        "service_op_save_line",
        "service_op_remove_line",
        "CreatePayRunLineBody",
        "dto_update::UpdatePayRunLineRequest",
    ] {
        assert!(
            handler.contains(needle),
            "the child action surface must carry `{needle}` before the build:\n{handler}"
        );
    }
    let router = fs::read_to_string(output.join("src/api/payroll/router.rs")).unwrap();
    assert!(
        router.contains("\"/actions/add_line\""),
        "the collection-level add route must emit:\n{router}"
    );

    codegraph::driver::check_generated_output(&output)
        .expect("the generated app with aggregate child operations must compile");
}
