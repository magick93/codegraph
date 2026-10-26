//! Compile gate for the gateway-hosted integration catalog surface
//! (issue #521): the generated gateway worker crate must type-check with
//! the catalog modules, the lean platform AppState, the shared worker auth
//! middleware, and the cornucopia client plumbing all in its Cargo graph.
//!
//! The generated `cornucopia-queries` crate is replaced by a check stub:
//! its real build.rs runs cornucopia codegen against a live migrated
//! Postgres (`CORNUCOPIA_DATABASE_URL`), which no CI lane owes this test.
//! The stub carries the `GenericClient` trait with the exact signature set
//! the generated `db_client.rs` implements, so the gateway's type checking
//! is faithful — only the query modules are absent (the gateway uses none).
//!
//! Ignored by default: downloads crates.io deps and compiles the gateway's
//! whole dependency tree (~1–2 min). Run in the ops-integration / nightly
//! lane:
//!
//! ```bash
//! cargo test -p codegraph --test workers_gateway_compile_tests -- --ignored --nocapture
//! ```

use std::path::Path;
use std::process::Command;

use codegraph::generate::template_engine;
use codegraph::profile::BuildPlan;

#[path = "test_framework/mod.rs"]
mod test_framework;

use test_framework::cornucopia_queries_stub::{
    BUILD_RS as STUB_BUILD_RS, CARGO_TOML as STUB_CARGO_TOML, LIB_RS as STUB_LIB_RS,
};

#[tokio::test]
#[ignore = "compiles a generated workers workspace: downloads crates.io deps, ~1-2 min"]
async fn workers_gateway_catalog_surface_compiles() {
    // ── Fixture: workers topology + extension points, cornucopia provider ──
    let pay_run = codegraph_core::types::SchemaNode {
        namespace: None,
        schema_id: "compensation/json/PayRunType.json".to_string(),
        title: "PayRunType".to_string(),
        description: Some("A pay run".to_string()),
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("compensation".to_string()),
        rel_path: "compensation/json/PayRunType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "PayRun".to_string(),
        pg_table_name: "pay_run".to_string(),
        api_path_segment: "pay-runs".to_string(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: true,
        has_one_of: false,
        has_any_of: false,
        has_definitions: true,
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
    };
    let props = vec![codegraph_core::types::PropertyNode {
        is_id: false,
        name: "name".to_string(),
        prop_type: "string".to_string(),
        description: Some("Name".to_string()),
        format: None,
        is_required: true,
        is_nullable: false,
        is_array: false,
        min_items: None,
        max_items: None,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: "name".to_string(),
        pg_column_type: "TEXT".to_string(),
        rust_field_name: "name".to_string(),
        rust_field_type: "String".to_string(),
        sea_orm_type: "Text".to_string(),
        render_strategy: "direct_column".to_string(),
        ref_target: None,
        classification: Some("primitive_wrapper".to_string()),
        projection: None,
        classification_kind: None,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
        type_expr: None,
    }];
    let mock = codegraph_core::mock::MockEngine::builder()
        .with_schema(pay_run)
        .with_properties("PayRunType", props)
        .build();
    let config = codegraph_config::config::parse_domain_config_str(
        r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.compensation]
label = "Compensation"
schema_dir = "compensation"
postgres_schema = "compensation"
entities = ["PayRunType"]
"#,
    )
    .unwrap();
    let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = template_engine::create_tera(&template_dir).unwrap();
    let output_dir = tempfile::TempDir::new().unwrap();
    let domain_types_tmp = tempfile::TempDir::new().unwrap();
    let hooks_tmp = tempfile::TempDir::new().unwrap();

    let ext = codegraph_ext_points::parse_extension_points_str(
        r#"
[payroll-sync]
name = "Payroll Sync"
description = "Synchronise pay data"
cardinality = "exclusive"
entities = ["compensation.pay_run"]
directions = ["push"]
"#,
    )
    .unwrap();

    // The workers workspace manifest path-depends on the repo's
    // codegraph-workflow / codegraph-type-contracts crates; Cargo resolves
    // the whole workspace when writing its lockfile, so point them at the
    // real checkout (absolute paths pass through `resolve_path` unchanged).
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();

    let project = codegraph::generate::ProjectConfig {
        identity: codegraph::generate::IdentityConfig {
            app_name: "compilecheck".to_string(),
            ..Default::default()
        },
        database: codegraph::generate::DatabaseConfig {
            database_target: codegraph::generate::db::dialect::DatabaseTarget::Postgres,
            persistence_provider: codegraph::profile::PersistenceProvider::Cornucopia,
        },
        deployment: codegraph::generate::DeploymentConfig {
            deployment_topology: codegraph::profile::DeploymentTopology::Workers,
        },
        integration: codegraph::generate::IntegrationFlags {
            has_extension_points: true,
            ..Default::default()
        },
        paths: codegraph::generate::PathsConfig {
            codegraph_workflow_base: repo
                .join("crates/codegraph-workflow")
                .to_string_lossy()
                .into_owned(),
            type_contracts_base: repo
                .join("crates/codegraph-type-contracts")
                .to_string_lossy()
                .into_owned(),
            ..Default::default()
        },
        ..Default::default()
    };

    let plan = BuildPlan {
        entity_generators: vec![
            "ddl".to_string(),
            "sea_orm_entity".to_string(),
            "handler".to_string(),
            "dto".to_string(),
            "repository".to_string(),
            "command".to_string(),
            "query".to_string(),
        ],
        domain_generators: vec![
            "errors".to_string(),
            "router".to_string(),
            "links".to_string(),
        ],
        global_generators: vec![
            "worker_scaffold".to_string(),
            // Emits the cornucopia-queries crate skeleton at the output root
            // (the worker/gateway Cargo manifests path-depend on it).
            "cornucopia_config".to_string(),
            "integration_tables".to_string(),
            "integration_config".to_string(),
            "integration_dispatch".to_string(),
            "integration_catalog".to_string(),
        ],
        post_gen_scripts: vec![],
        ifml_frameworks: vec![],
        template_pack_path: None,
        database_target: codegraph::generate::db::dialect::DatabaseTarget::Postgres,
        has_atproto: false,
        atproto_tenancy: "shared_pds".to_string(),
        has_fern: false,
        has_emdash: false,
        fern_sdk_languages: vec!["typescript".to_string()],
        persistence_provider: codegraph::profile::PersistenceProvider::Cornucopia,
        dto_key_casing: "snake".to_string(),
        deployment_topology: codegraph::profile::DeploymentTopology::Workers,
        dependency_strategy: codegraph::profile::DependencyStrategy::Rev,
        namespace_layout: false,
        ux_rules: false,
        expr_ir: false,
        ifml_e2e_auth: false,
        testid_module: false,
        structural_gates: false,
        public_operations_rls: false,
        features: toml::Table::new(),
    };

    let report =
        codegraph::generate::run_generators_with_opts(codegraph::generate::GeneratorOpts {
            db: &mock,
            config: &config,
            output_dir: output_dir.path(),
            tera: &tera,
            ui_overrides: &Default::default(),
            ui_domains: &Default::default(),
            schema_base_dir: Path::new(""),
            domain_types_base: Some(domain_types_tmp.path()),
            hooks_base: Some(hooks_tmp.path()),
            ext_points: Some(&ext),
            seed_config: None,
            build_plan: Some(&plan),
            ifml_frameworks: vec![],
            ifml_components: None,
            ux_rules: None,
            project_config: Some(&project),
            emdash_plugins: None,
            domain_config_dir: None,
        })
        .await
        .unwrap();
    assert!(
        !report.has_errors(),
        "generation should succeed: {}",
        report.summary()
    );

    // The gateway must have the full catalog surface on disk.
    for required in [
        "workers/gateway/src/integration_catalog.rs",
        "workers/gateway/src/integration_router.rs",
        "workers/gateway/src/integration_config.rs",
        "workers/gateway/src/app_state.rs",
        "workers/gateway/src/db_client.rs",
        "workers/gateway/src/middleware.rs",
        "workers/gateway/src/error.rs",
    ] {
        let path = output_dir.path().join(required);
        assert!(path.exists(), "missing generated file: {required}");
    }

    // Replace the cornucopia-queries crate with the check stub (see module
    // doc): no live-Postgres codegen, same trait surface.
    let queries_dir = output_dir.path().join("cornucopia-queries");
    assert!(
        queries_dir.join("Cargo.toml").exists(),
        "cornucopia_config must emit the cornucopia-queries crate skeleton"
    );
    std::fs::write(queries_dir.join("Cargo.toml"), STUB_CARGO_TOML).unwrap();
    std::fs::write(queries_dir.join("build.rs"), STUB_BUILD_RS).unwrap();
    std::fs::write(queries_dir.join("src").join("lib.rs"), STUB_LIB_RS).unwrap();

    // ── cargo check -p {app}-gateway ─────────────────────────────────────
    let gateway_crate = "compilecheck-gateway";
    let output = Command::new("cargo")
        .arg("check")
        .arg("-p")
        .arg(gateway_crate)
        .arg("--manifest-path")
        .arg(output_dir.path().join("workers").join("Cargo.toml"))
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .expect("failed to spawn cargo check");

    if !output.status.success() {
        panic!(
            "cargo check -p {gateway_crate} failed (status {:?}):\n--- stdout ---\n{}\n--- stderr ---\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
