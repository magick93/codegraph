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
//! The injected-scope `when` leg (issue #663) also lives here: every leg in
//! this binary now runs against a PER-RUN type registry (issue #746), so
//! fixtures that declare the same class titles (`PayRunType`) in different
//! domains no longer resolve each other's module paths.
//!
//! History: that leg originally shipped as its own test binary
//! (`service_ops_injected_guard_gate.rs`) — the workaround for the
//! process-global `TYPE_REGISTRY` shared by every `driver::run` in one
//! process. #746 retired the workaround (the registry lives on
//! `GeneratorContext` per run) and merged the leg back here, where it is
//! the isolation proof for the payroll/release fixtures.
//!
//! ```text
//! cargo test -p codegraph --test service_ops_compile_gate -- --ignored --nocapture
//! ```

#[path = "service_ops_fixture.rs"]
mod fixture;

use std::fs;
use std::path::{Path, PathBuf};

use fixture::{
    DOMAINS_TOML, PAY_RUN_MOX, PAY_RUN_SCHEMA, PAYROLL_DDD, gate_tempdir, ingest, ingest_guard,
    write_fixture_in, write_guard_fixture,
};

/// The fixture profiles: the workspace default with path deps (the gate
/// must compile without network) and the gRPC generators stripped (no
/// protoc requirement). Also pins `type_contracts_base` at the local
/// checkout: without it the generated domain-types crate pins
/// `codegraph-type-contracts` at this checkout's (unpushed) git rev and
/// the gate's cargo check fetch fails (the WS1 finding).
fn write_profiles(dir: &Path) -> std::path::PathBuf {
    write_profiles_inner(dir, false)
}

/// The invariant variant (issue #666 slices 4d+4e): the plan ALSO carries
/// the `rules` generator (with the `rosetta_backend` feature it requires)
/// — the enforcement plane is an opt-in like the #338 guards, and the
/// review-fixture regen is the established injection precedent.
fn write_invariant_profiles(dir: &Path) -> std::path::PathBuf {
    write_profiles_inner(dir, true)
}

fn write_profiles_inner(dir: &Path, with_rules: bool) -> std::path::PathBuf {
    write_profiles_with(dir, with_rules, "")
}

/// [`write_profiles_inner`] plus extra `[features]` lines (issue #635: the
/// agent-mode leg injects `api_key_mode = "agent"`).
fn write_profiles_with(dir: &Path, with_rules: bool, extra_features: &str) -> std::path::PathBuf {
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
        &format!("[profiles.default.features]\ndependency_strategy = \"path\"\n{extra_features}"),
    );
    if with_rules {
        // The workspace profile declares `rosetta_backend = false` — flip
        // it (a duplicate key is a parse error).
        text = text.replace("rosetta_backend = false", "rosetta_backend = true");
    }
    // Strip grpc generator lines (protoc is not a gate prerequisite here).
    let filtered: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim_start().starts_with("\"grpc_"))
        .collect();
    let mut body = filtered.join("\n");
    if with_rules {
        // The `rules` domain generator rides the [api] list (the
        // eligibility-guard injection shape from `review_api_suite_regen`).
        body = body.replace(
            "    \"errors\", \"router\", \"links\",",
            "    \"errors\", \"router\", \"links\", \"rules\",",
        );
    }
    let path = dir.join("profiles.toml");
    fs::write(&path, body).unwrap();
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

    for (entity, target) in [("release", "SignedOff"), ("review", "Cleared")] {
        let handler =
            fs::read_to_string(output.join(format!("src/api/release/{entity}_handler.rs")))
                .unwrap();
        // The kernel probes (issue #818): the authored binding bakes the
        // op's declared target — the match arms are retired.
        assert!(
            handler.contains(&format!(r#"let legal: Vec<&str> = ["{target}"]"#)),
            "the authored binding must bake its declared kernel-probe target before the build: \
             {target}\n{handler}"
        );
        assert!(
            handler.contains("codegraph_workflow::MachineRuntime::from_json(&machine_ir)"),
            "the authored binding must route through the MachineRuntime: {entity}\n{handler}"
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

/// Issue #666 slices 4d+4e (the #551 policy): the design-invariant plane
/// compiles — the slice-2 predicate fns in `rules.rs`, the create
/// compensation + candidate pre-write blocks, the member validation's
/// scoped member fetches, and the transition-arm invariant block compile
/// together with the rest of the app. Before this pin the invariant call
/// sites compiled NOWHERE (the silent-emission class the policy exists
/// for).
#[tokio::test]
#[ignore = "slow: cargo check over the generated app; needs the full dep stack"]
async fn generated_app_with_design_invariants_compiles() {
    let fixture = fixture::write_invariant_fixture();
    let _engine = fixture::ingest_invariant(&fixture).await;

    let out_dir = gate_tempdir();
    let output = out_dir.path().join("app");
    fs::create_dir_all(&output).unwrap();
    let fixture_root = fixture.config.parent().unwrap().to_path_buf();
    let profiles_path = write_invariant_profiles(&fixture_root);

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
    .expect("generation over the invariant fixture must succeed");

    // The invariant surface must be ON DISK before any build spend: the
    // rules predicates, the candidate pre-write blocks, the member
    // validation, and the transition arm's invariant block.
    let rules = fs::read_to_string(output.join("src/domain/payroll/rules.rs")).unwrap();
    for needle in [
        "pub fn pay_run_type_invariant_0(input: &PayRunResponse) -> bool {",
        "pub fn pay_run_line_type_invariant_0(input: &PayRunLineResponse) -> bool {",
    ] {
        assert!(
            rules.contains(needle),
            "the predicate fns must emit before the build: {needle}\n{rules}"
        );
    }
    let handler = fs::read_to_string(output.join("src/api/payroll/pay_run_handler.rs")).unwrap();
    for needle in [
        "code: \"INVARIANT_VIOLATION\"",
        "let candidate: PayRunResponse = serde_json::from_value(candidate_json)",
        "pay_run_type_invariant_0(&candidate)",
        "pay_run_type_invariant_0(&response)",
        "payroll_pay_run_line_queries.list_filtered(0, 1000, &member_filters_pay_run_line",
        "pay_run_line_type_invariant_0(&member_row)",
        "design invariant violated on create — compensating",
    ] {
        assert!(
            handler.contains(needle),
            "the root handler must carry `{needle}` before the build:\n{handler}"
        );
    }
    let child_handler =
        fs::read_to_string(output.join("src/api/payroll/pay_run_line_handler.rs")).unwrap();
    for needle in [
        "let existing = state.payroll_pay_run_line_queries.find_by_id_scoped(id, parent_id",
        "pay_run_line_type_invariant_0(&candidate)",
    ] {
        assert!(
            child_handler.contains(needle),
            "the child handler must carry `{needle}` before the build:\n{child_handler}"
        );
    }

    codegraph::driver::check_generated_output(&output)
        .expect("the generated app with design invariants must pass the compile gate");
}

/// Issue #663 (the #551 policy): the injected-scope `when` plane compiles —
/// the async authorize fn (`&AppState` + `&ApiKeyInfo` params, the
/// own-record FK extraction, the sibling query slot's `find_by_id` load,
/// and the scoped comparison) compiles against the generated app exactly
/// like the synchronous own-record gate does.
///
/// Issue #746: this leg lives in the SAME binary as the payroll/release legs
/// on purpose — the guard fixture declares `PayRunType` in `guardpay` while
/// the payroll fixture declares it in `payroll`, so it is the proof that
/// per-run type registries keep sibling generations isolated.
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

/// Issue #767: the COLLECTION-LEVEL create body compiles. The dto-boundary
/// fixture's `registerBook(NewBookDraft)` is create-shaped — it lowers to
/// `POST …/{entity}/actions/{op}` whose body IS the modeled boundary — and the
/// handler CONSTRUCTS `CreateBookRequest` from it field by field. A misspelt
/// DTO field, a wrong `payload` type, or a stale create-command arity is a
/// compile error here, not a silent runtime 422 (the #551 policy: this plane
/// emits Rust, so it ships a test that compiles it).
#[tokio::test]
#[ignore = "slow: cargo check over the generated app; needs the full dep stack"]
async fn generated_app_with_a_collection_level_create_body_compiles() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dto_boundaries");
    let out_dir = gate_tempdir();
    let output = out_dir.path().join("app");
    fs::create_dir_all(&output).unwrap();
    // The fixture carries no domains.toml of its own (the pipeline test writes
    // its config inline); the gate does the same.
    let config = out_dir.path().join("domains.toml");
    fs::write(
        &config,
        r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.library]
label = "Library"
schema_dir = "schemas/library"
postgres_schema = "library"
"#,
    )
    .unwrap();
    let profiles_path = write_profiles(out_dir.path());
    let mox_files = vec![fixture.join("dto_boundaries.mox")];
    let ddd_files = vec![fixture.join("dto_boundaries.ddd")];

    codegraph::driver::run(codegraph::driver::RunArgs {
        schemas: None,
        classifier: None,
        config_path: &config,
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
        mox_files: &mox_files,
        rosetta_files: &[],
        ddd_files: &ddd_files,
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
    .expect("generation over the dto-boundary fixture must succeed");

    // Pin the emission before the build spend.
    let handler = fs::read_to_string(output.join("src/api/library/book_handler.rs"))
        .expect("the Book handler must emit");
    assert!(
        handler.contains("/library/book/actions/register_book"),
        "the create-shaped route must be collection-level:\n{handler}"
    );
    assert!(
        handler.contains("let item = CreateBookRequest {")
            && handler.contains("title: payload.title,")
            && handler.contains("play_length: payload.play_length,"),
        "the boundary must construct the create DTO field by field:\n{handler}"
    );

    codegraph::driver::check_generated_output(&output)
        .expect("the generated app with a collection-level create body must compile");
}

/// Issue #768: the PROJECTION READ plane compiles — the item-addressed and
/// collection-addressed endpoints (handlers constructing the read model
/// through the boundary's `from_row`), the collection router mount, the
/// finder plane, and the OpenAPI `body = domain_types::dto::…` spell all
/// compile together with the rest of the app. The dto_boundaries fixture
/// carries both shapes (`summarise(Book book)` + `BookSummary[0..*]
/// allSummaries();`) over the same `BookSummary from Book` boundary the
/// node-free pins drive.
#[tokio::test]
#[ignore = "slow: cargo check over the generated app; needs the full dep stack"]
async fn generated_app_with_projection_reads_compiles() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture = manifest_dir.join("tests/fixtures/dto_boundaries");
    let out_dir = gate_tempdir();
    let output = out_dir.path().join("app");
    fs::create_dir_all(&output).unwrap();
    let fixture_root = out_dir.path().to_path_buf();
    let config = fixture_root.join("domains.toml");
    fs::write(
        &config,
        r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.library]
label = "Library"
schema_dir = "schemas/library"
postgres_schema = "library"
"#,
    )
    .unwrap();
    let profiles_path = write_profiles(&fixture_root);

    codegraph::driver::run(codegraph::driver::RunArgs {
        schemas: None,
        classifier: None,
        config_path: &config,
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
        mox_files: &[fixture.join("dto_boundaries.mox")],
        rosetta_files: &[],
        ddd_files: &[fixture.join("dto_boundaries.ddd")],
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
    .expect("generation over the dto_boundaries fixture must succeed");

    // The endpoints must be ON DISK before any build spend: both handlers
    // construct through the boundary constructor, the collection route
    // mounts at the collection level, and the actions contract asserts the
    // derived member against the seeded row.
    let handler = fs::read_to_string(output.join("src/api/library/book_handler.rs"))
        .expect("the book handler must emit");
    assert!(
        handler.contains("service_op_summarise"),
        "the item projection read must emit:\n{handler}"
    );
    assert!(
        handler.contains("BookSummary::from_row(&response)"),
        "the item read must construct through from_row:\n{handler}"
    );
    assert!(
        handler.contains("service_op_all_summaries"),
        "the collection projection read must emit:\n{handler}"
    );
    assert!(
        handler.contains("rows.iter().map(domain_types::dto::catalog::BookSummary::from_row)"),
        "the collection read must map the collection through from_row:\n{handler}"
    );
    let router = fs::read_to_string(output.join("src/api/library/router.rs"))
        .expect("the library router must emit");
    assert!(
        router.contains("/{book_id}/actions/summarise"),
        "the item read route must mount item-addressed:\n{router}"
    );
    assert!(
        router.contains("/actions/all_summaries"),
        "the collection read route must mount at the collection level:\n{router}"
    );
    let hurl_dir = output.join("hurl");
    let actions = fs::read_dir(&hurl_dir)
        .expect("the generated hurl dir must exist")
        .flatten()
        .map(|e| e.path())
        .find(|p| p.to_string_lossy().ends_with("_library_book_actions.hurl"))
        .expect("the book actions contract must emit");
    let actions = fs::read_to_string(actions).unwrap();
    assert!(
        actions.contains("\"$.data.minutes\" == 0"),
        "the derived member must be asserted against the seeded row (play_length seeds 42; \
         integer division 42/60 = 0):\n{actions}"
    );

    codegraph::driver::check_generated_output(&output)
        .expect("the generated app with projection reads must compile");
}

/// Copy the committed machine read-model shapes fixture
/// (`tests/fixtures/machine_read_shapes`: the model keeps its `model/` and
/// `design/` layout because the design imports `../model/recruiting.mox`).
/// Returns `(domains.toml, mox, ddd)`.
fn write_read_shapes_fixture(root: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/machine_read_shapes");
    let mox = root.join("model/recruiting.mox");
    let ddd = root.join("design/recruiting.ddd");
    let config = root.join("domains.toml");
    fs::create_dir_all(mox.parent().unwrap()).unwrap();
    fs::create_dir_all(ddd.parent().unwrap()).unwrap();
    fs::copy(src.join("model/recruiting.mox"), &mox).unwrap();
    fs::copy(src.join("design/recruiting.ddd"), &ddd).unwrap();
    fs::copy(src.join("domains.toml"), &config).unwrap();
    (config, mox, ddd)
}

/// Issues #817 + #824: the machine read model compiles for BOTH ordinary
/// shapes. The fixture carries a machine on a design WITHOUT a repository
/// and a machine bound to a `…Code`-typed status field on a root with one —
/// the two shapes whose route module failed a consumer's `cargo check`
/// (`E0599`: no method named `find_by_id`; `E0609`: no field
/// `approval_status_code`) while every gate stayed green, because no
/// compile pin reached them: every fixture put its machines on root
/// scaffold entities with plain `*_status` fields, and the route is
/// monolith-only, so on workers it is unrouted code that merely has to
/// compile.
///
/// The node-free pins (`machine_read_shape_tests`) hold the emitted
/// shape; this leg is the whole-app `cargo check` over the workspace
/// default profile (path deps) that proves the emitted read model compiles
/// against the DTO and the query plane it addresses.
#[tokio::test]
#[ignore = "slow: cargo check over the generated app; needs the full dep stack"]
async fn generated_app_with_machine_read_shapes_compiles() {
    let out_dir = gate_tempdir();
    let output = out_dir.path().join("app");
    fs::create_dir_all(&output).unwrap();
    let fixture_root = out_dir.path().join("fixture");
    fs::create_dir_all(&fixture_root).unwrap();
    let (config, mox, ddd) = write_read_shapes_fixture(&fixture_root);
    let profiles_path = write_profiles(&fixture_root);

    codegraph::driver::run(codegraph::driver::RunArgs {
        schemas: None,
        classifier: None,
        config_path: &config,
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
        mox_files: std::slice::from_ref(&mox),
        rosetta_files: &[],
        ddd_files: std::slice::from_ref(&ddd),
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
    .expect("generation over the machine read-model shapes fixture must succeed");

    // The emitted shape, pinned before any build spend (a regression here
    // must not burn a cargo check to be discovered).
    assert!(
        !output
            .join("src/api/recruiting/application_machine.rs")
            .exists(),
        "a machine without a loader must emit no read-model module (issue #824)"
    );
    let module = fs::read_to_string(output.join("src/api/recruiting/position_opening_machine.rs"))
        .expect("the `…Code` machine keeps its read-model module (issue #824)");
    assert!(
        module.contains("entity.approval_status.to_string()")
            && !module.contains("entity.approval_status_code"),
        "the read model must address the DTO's own field name:\n{module}"
    );

    codegraph::driver::check_generated_output(&output)
        .expect("the generated app with both machine read-model shapes must compile");
}

/// Issue #635: the generated app compiles in EVERY accepted API-key mode,
/// table-driven. Unlike the legs above, the actor policy reaches the graph
/// (policies ride `.ifml` imports only — the `.ifml` shim carries both
/// policy files and declares no UI framework, so no IFML UI emits), so the
/// human role arms, the `never_both` denial arms and — in agent mode — the
/// `"api_key"` arm are all rendered from real grants and compiled, with the
/// `app.session_kind` bind in both session bundles and the kind-keyed role
/// gate. A new accepted mode value adds a row here.
#[tokio::test]
#[ignore = "slow: cargo check over the generated app per mode; needs the full dep stack"]
async fn generated_app_compiles_in_every_api_key_mode() {
    struct Row {
        label: &'static str,
        features: &'static str,
        /// Use the kind-bearing policy (`permit human` / `permit agent`).
        kind_policy: bool,
        /// Markers the generated handler must carry before the build spend.
        handler_markers: &'static [&'static str],
    }
    let rows = [
        Row {
            label: "bypass",
            features: "",
            kind_policy: false,
            handler_markers: &[r#"if role == "api_key""#, r#""owner" => {"#],
        },
        Row {
            label: "agent × unmodified any",
            features: "api_key_mode = \"agent\"\n",
            kind_policy: false,
            handler_markers: &[
                r#""api_key" => {"#,
                "separation of duties: role `api_key` holds",
            ],
        },
        Row {
            label: "agent × unmodified human, kind-bearing policy",
            features: "api_key_mode = \"agent\"\nunmodified_grants = \"human\"\n",
            kind_policy: true,
            handler_markers: &[
                r#""api_key" => {"#,
                "is not granted to agent sessions",
                r#""owner" => {"#,
            ],
        },
    ];
    for row in &rows {
        let fixture = write_fixture_in("service_ops_mode_gate", PAYROLL_DDD);
        if row.kind_policy {
            fs::write(&fixture.actor, fixture::KIND_POLICY_ACTOR).unwrap();
        }
        let fixture_root = fixture.config.parent().unwrap().to_path_buf();
        let shim = fixture_root.join("policy_shim.ifml");
        fs::write(
            &shim,
            "import \"policy.actor\";\nimport \"policy_b.actor\";\n\ndomain \"payroll\" { schema \"payroll\"; }\n",
        )
        .unwrap();
        let profiles_path = write_profiles_with(&fixture_root, false, row.features);
        let out_dir = gate_tempdir();
        let output = out_dir.path().join("app");
        fs::create_dir_all(&output).unwrap();

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
            ifml_files: std::slice::from_ref(&shim),
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
        .unwrap_or_else(|e| panic!("{}: generation must succeed: {e}", row.label));

        let handler =
            fs::read_to_string(output.join("src/api/payroll/pay_run_handler.rs")).unwrap();
        for marker in row.handler_markers {
            assert!(
                handler.contains(marker),
                "{}: the handler must carry `{marker}` before the build spend",
                row.label
            );
        }
        codegraph::driver::check_generated_output(&output)
            .unwrap_or_else(|e| panic!("{}: the generated app must compile: {e}", row.label));
    }
}
