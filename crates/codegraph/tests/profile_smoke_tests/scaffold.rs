use std::path::Path;

use codegraph::generate::template_engine;
use codegraph::profile::BuildPlan;
use codegraph_core::types::{PropertyNode, SchemaNode};

use crate::harness::run_routing_generators_with_parts;

/// Two-domain mock: compensation (with worker config keys + depends_on) and
/// common (with cron triggers). Mirrors `mock_test_setup` with extra domains
/// so the worker scaffold's service-binding and cron rendering is exercised.
fn workers_scaffold_test_setup() -> (
    codegraph_core::mock::MockEngine,
    codegraph_config::DomainConfig,
    tera::Tera,
    tempfile::TempDir,
) {
    let pay_run = SchemaNode {
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
    let code = SchemaNode {
        namespace: None,
        schema_id: "common/json/CodeType.json".to_string(),
        title: "CodeType".to_string(),
        description: Some("A code value".to_string()),
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("common".to_string()),
        rel_path: "common/json/CodeType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "Code".to_string(),
        pg_table_name: "code".to_string(),
        api_path_segment: "codes".to_string(),
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

    let props = vec![PropertyNode {
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

    let engine = codegraph_core::mock::MockEngine::builder()
        .with_schema(pay_run)
        .with_schema(code)
        .with_properties("PayRunType", props.clone())
        .with_properties("CodeType", props)
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
worker_name = "hr-payroll-worker"
hyperdrive_binding = "PAYROLL_DB"
depends_on = ["common"]

[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"
entities = ["CodeType"]
cron_triggers = ["0 0 * * *"]
"#,
    )
    .unwrap();

    let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = template_engine::create_tera(&template_dir).unwrap();

    let output_dir = tempfile::TempDir::new().unwrap();

    (engine, config, tera, output_dir)
}

/// Workers-topology run asserting the worker scaffold produces the full
/// per-domain + workspace + gateway file set, and that the wrangler.toml files
/// parse as TOML with the expected hyperdrive/service-binding entries.
#[tokio::test]
async fn workers_topology_generates_worker_scaffold_and_gateway() {
    let (mock, config, tera, output_dir) = workers_scaffold_test_setup();
    let domain_types_tmp = tempfile::TempDir::new().unwrap();
    let hooks_tmp = tempfile::TempDir::new().unwrap();

    let plan = BuildPlan {
        entity_form: false,
        ifml_tanstack_forms: false,
        entity_generators: vec![
            "ddl".to_string(),
            "sea_orm_entity".to_string(),
            "handler".to_string(),
            "dto".to_string(),
            "repository".to_string(),
            "command".to_string(),
            "query".to_string(),
            "event".to_string(),
        ],
        domain_generators: vec![
            "errors".to_string(),
            "router".to_string(),
            "links".to_string(),
        ],
        global_generators: vec!["worker_scaffold".to_string()],
        post_gen_scripts: vec![],
        ifml_frameworks: vec![],
        template_pack_path: None,
        database_target: codegraph::generate::db::dialect::DatabaseTarget::Postgres,
        has_atproto: false,
        atproto_tenancy: "shared_pds".to_string(),
        has_fern: false,
        has_emdash: false,
        fern_sdk_languages: vec!["typescript".to_string()],
        persistence_provider: codegraph::profile::PersistenceProvider::SeaOrm,
        dto_key_casing: "snake".to_string(),
        deployment_topology: codegraph::profile::DeploymentTopology::Workers,
        dependency_strategy: codegraph::profile::DependencyStrategy::Rev,
        namespace_layout: false,
        ux_rules: false,
        expr_ir: false,
        ifml_e2e_auth: false,
        ifml_remote_functions: false,
        entity_remote_functions: false,
        testid_module: false,
        structural_gates: false,
        public_operations_rls: false,
        features: toml::Table::new(),
    };

    let project = codegraph::generate::ProjectConfig {
        deployment: codegraph::generate::DeploymentConfig {
            deployment_topology: codegraph::profile::DeploymentTopology::Workers,
        },
        ..Default::default()
    };

    let report =
        codegraph::generate::run_generators_with_opts(codegraph::generate::GeneratorOpts {
            integration_manifests: None,
            db: &mock,
            config: &config,
            output_dir: output_dir.path(),
            tera: &tera,
            ui_overrides: &Default::default(),
            ui_domains: &Default::default(),
            schema_base_dir: Path::new(""),
            domain_types_base: Some(domain_types_tmp.path()),
            hooks_base: Some(hooks_tmp.path()),
            ext_points: None,
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
        .expect("workers scaffold run should succeed");

    assert!(
        !report.has_errors(),
        "workers scaffold run should have no errors: {}",
        report.summary()
    );

    let paths: Vec<String> = report
        .files
        .iter()
        .map(|f| f.path.to_string_lossy().to_string())
        .collect();
    let any = |needle: &str| paths.iter().any(|p| p.contains(needle));

    // Per-domain worker crates: both domains with entities get one.
    for domain in ["common", "compensation"] {
        let base = format!("/workers/{domain}");
        assert!(any(&format!("{base}/Cargo.toml")), "{domain} Cargo.toml");
        assert!(any(&format!("{base}/src/main.rs")), "{domain} main.rs");
        assert!(any(&format!("{base}/src/lib.rs")), "{domain} lib.rs");
        assert!(any(&format!("{base}/src/worker.rs")), "{domain} worker.rs");
        assert!(
            any(&format!("{base}/src/app_state.rs")),
            "{domain} app_state.rs"
        );
        assert!(any(&format!("{base}/src/error.rs")), "{domain} error.rs");
        assert!(
            any(&format!("{base}/src/middleware.rs")),
            "{domain} middleware.rs"
        );
        assert!(
            any(&format!("{base}/src/qs_query.rs")),
            "{domain} qs_query.rs"
        );
        assert!(
            any(&format!("{base}/wrangler.toml")),
            "{domain} wrangler.toml"
        );
    }

    // Workspace manifest + gateway crate.
    assert!(any("/workers/Cargo.toml"), "workspace manifest");
    assert!(any("/workers/gateway/Cargo.toml"), "gateway Cargo.toml");
    assert!(any("/workers/gateway/src/main.rs"), "gateway main.rs");
    assert!(any("/workers/gateway/src/lib.rs"), "gateway lib.rs");
    assert!(any("/workers/gateway/src/worker.rs"), "gateway worker.rs");
    assert!(any("/workers/gateway/src/cors.rs"), "gateway cors.rs");
    assert!(
        any("/workers/gateway/wrangler.toml"),
        "gateway wrangler.toml"
    );
    // The CORS layer rides both runtimes and honors CORS_ALLOWED_ORIGINS.
    let gateway_main = report
        .files
        .iter()
        .find(|f| f.path.ends_with("workers/gateway/src/main.rs"))
        .map(|f| f.content.clone())
        .unwrap_or_default();
    assert!(
        gateway_main.contains("CORS_ALLOWED_ORIGINS")
            && gateway_main.contains("crate::cors::Cors::resolve"),
        "native gateway must wire the CORS layer"
    );

    // Workspace manifest lists gateway + one member per domain.
    let workspace_manifest = report
        .files
        .iter()
        .find(|f| f.path.ends_with("workers/Cargo.toml"))
        .unwrap();
    let workspace_toml: toml::Value =
        toml::from_str(&workspace_manifest.content).expect("workspace manifest must parse");
    let members = workspace_toml["workspace"]["members"]
        .as_array()
        .expect("workspace members array");
    let member_names: Vec<&str> = members.iter().map(|m| m.as_str().unwrap()).collect();
    assert!(member_names.contains(&"gateway"));
    assert!(member_names.contains(&"common"));
    assert!(member_names.contains(&"compensation"));
    assert!(
        workspace_toml["workspace"]["dependencies"]
            .get("worker")
            .is_some(),
        "workspace deps must pin the worker crate"
    );

    // Compensation worker wrangler: explicit worker name, hyperdrive binding,
    // and a [[services]] entry for its depends_on domain (binding uppercase,
    // service = the common worker's resolved name).
    let compensation_wrangler = report
        .files
        .iter()
        .find(|f| f.path.ends_with("workers/compensation/wrangler.toml"))
        .unwrap();
    let wrangler_toml: toml::Value = toml::from_str(&compensation_wrangler.content)
        .expect("compensation wrangler.toml must parse as TOML");
    assert_eq!(wrangler_toml["name"].as_str(), Some("hr-payroll-worker"));
    assert!(
        wrangler_toml["build"]["command"]
            .as_str()
            .unwrap()
            .contains("--features cloudflare-worker")
    );
    assert_eq!(
        wrangler_toml["hyperdrive"][0]["binding"].as_str(),
        Some("PAYROLL_DB"),
        "compensation wrangler must declare its PAYROLL_DB hyperdrive binding"
    );
    assert_eq!(
        wrangler_toml["services"][0]["binding"].as_str(),
        Some("COMMON"),
        "depends_on fallback must produce a COMMON service binding"
    );
    assert_eq!(
        wrangler_toml["services"][0]["service"].as_str(),
        Some("app-common"),
        "service name must be the common worker's resolved name (app-common)"
    );

    // Common worker wrangler: default worker name + cron triggers.
    let common_wrangler = report
        .files
        .iter()
        .find(|f| f.path.ends_with("workers/common/wrangler.toml"))
        .unwrap();
    let common_toml: toml::Value =
        toml::from_str(&common_wrangler.content).expect("common wrangler.toml must parse");
    assert_eq!(common_toml["name"].as_str(), Some("app-common"));
    assert_eq!(
        common_toml["triggers"]["crons"][0].as_str(),
        Some("0 0 * * *"),
        "common wrangler must carry its cron trigger"
    );
    assert_eq!(
        common_toml["hyperdrive"][0]["binding"].as_str(),
        Some("HYPERDRIVE"),
        "common wrangler must default to the HYPERDRIVE binding"
    );

    // Gateway wrangler: one [[services]] entry per domain worker.
    let gateway_wrangler = report
        .files
        .iter()
        .find(|f| f.path.ends_with("workers/gateway/wrangler.toml"))
        .unwrap();
    let gateway_toml: toml::Value =
        toml::from_str(&gateway_wrangler.content).expect("gateway wrangler.toml must parse");
    assert_eq!(gateway_toml["name"].as_str(), Some("app-gateway"));
    let services = gateway_toml["services"]
        .as_array()
        .expect("gateway services array");
    assert_eq!(services.len(), 2, "one gateway service binding per domain");
    let bindings: Vec<&str> = services
        .iter()
        .map(|s| s["binding"].as_str().unwrap())
        .collect();
    assert!(bindings.contains(&"COMMON"));
    assert!(bindings.contains(&"COMPENSATION"));

    // Gateway forwards each domain with the /api/v1/{domain}/*path pattern.
    let gateway_worker = report
        .files
        .iter()
        .find(|f| f.path.ends_with("workers/gateway/src/worker.rs"))
        .unwrap();
    assert!(
        gateway_worker.content.contains("/api/v1/{domain}/{*path}"),
        "gateway must route domains with the prefix-stripping wildcard"
    );
    assert!(
        gateway_worker.content.contains("env.service"),
        "gateway must fetch via service bindings"
    );

    // Files were actually written to disk (not just reported).
    assert!(output_dir.path().join("workers/Cargo.toml").exists());
    assert!(
        output_dir
            .path()
            .join("workers/compensation/src/worker.rs")
            .exists()
    );
    assert!(
        output_dir
            .path()
            .join("workers/gateway/wrangler.toml")
            .exists()
    );

    println!("workers scaffold produced {} files", report.files.len());
}

/// Workers topology with hooks enabled: each worker gets a `src/hooks/`
/// module re-exporting the shared hooks-api registry (worker_app_state
/// references `crate::hooks::HookRegistry`), a per-worker `src/api/meta.rs`
/// (routed handlers reference `crate::api::meta::Meta`), and a
/// metrics-middleware point in the worker middleware module.  The monolith
/// topology keeps its root-only layout.
#[tokio::test]
async fn workers_topology_emits_hooks_reexport_and_api_meta() {
    let (mock, config, tera, output_dir) = workers_scaffold_test_setup();

    let project = codegraph::generate::ProjectConfig {
        deployment: codegraph::generate::DeploymentConfig {
            deployment_topology: codegraph::profile::DeploymentTopology::Workers,
        },
        identity: codegraph::generate::IdentityConfig {
            hooks_api_crate: "hr_hooks_api".to_string(),
            ..Default::default()
        },
        paths: codegraph::generate::PathsConfig {
            hooks_api_base: "crates/hr-hooks-api".to_string(),
            ..Default::default()
        },
        ..Default::default()
    };
    let (report, output_dir) = run_routing_generators_with_parts(
        mock,
        config,
        tera,
        output_dir,
        codegraph::profile::DeploymentTopology::Workers,
        project,
    )
    .await;

    let find = |needle: &str| {
        report
            .files
            .iter()
            .find(|f| f.path.to_string_lossy().ends_with(needle))
            .unwrap_or_else(|| panic!("missing generated file: {needle}"))
    };

    // Hooks: per-worker re-export of the shared registry.
    let hooks_mod = find("workers/compensation/src/hooks/mod.rs");
    assert!(
        hooks_mod
            .content
            .contains("pub use hr_hooks_api::generated::registry::HookRegistry;"),
        "worker hooks mod must re-export the shared HookRegistry, got:\n{}",
        hooks_mod.content
    );
    assert!(
        find("workers/compensation/src/lib.rs")
            .content
            .contains("pub mod hooks;")
    );
    assert!(
        find("workers/compensation/src/main.rs")
            .content
            .contains("mod hooks;")
    );
    assert!(
        find("workers/compensation/src/app_state.rs")
            .content
            .contains("use crate::hooks::HookRegistry;")
    );
    // The worker manifest carries a path dependency on the hooks-api crate so
    // the `hooks_api::...` references in command/query/app_state compile. The
    // relative path must resolve from the worker crate dir to the hooks crate.
    let worker_cargo = find("workers/compensation/Cargo.toml");
    let dep_line = worker_cargo
        .content
        .lines()
        .find(|l| l.contains("hr-hooks-api = { path ="))
        .unwrap_or_else(|| {
            panic!(
                "worker Cargo.toml missing hooks-api path dependency, got:\n{}",
                worker_cargo.content
            )
        });
    let q0 = dep_line.find('"').expect("quoted path");
    let rest = &dep_line[q0 + 1..];
    let q1 = rest.find('"').expect("closing quote");
    let dep_path = Path::new(&rest[..q1]);
    let worker_dir = output_dir.path().join("workers/compensation");
    let resolved = normalize_abs(&worker_dir.join(dep_path));
    let expected = normalize_abs(
        &std::env::current_dir()
            .expect("cwd")
            .join("crates/hr-hooks-api"),
    );
    assert_eq!(
        resolved,
        expected,
        "worker hooks-api dep must resolve to the hooks crate from the worker \
         crate dir, got path `{}` (did not account for workers/{{domain}} nesting)",
        dep_path.display()
    );
    // Common worker gets the same hooks module.
    assert!(report.files.iter().any(|f| {
        f.path
            .to_string_lossy()
            .ends_with("workers/common/src/hooks/mod.rs")
    }));

    // API meta: per-worker copy of the monolith meta.tera.
    let meta = find("workers/compensation/src/api/meta.rs");
    assert!(
        meta.content.contains("pub struct Meta"),
        "worker api/meta.rs must define Meta, got:\n{}",
        meta.content
    );

    // Metrics middleware point: entities with pipeline.middleware =
    // ["metrics"] must compile against the worker middleware module.
    let middleware = find("workers/compensation/src/middleware.rs");
    assert!(
        middleware.content.contains("pub mod metrics_middleware")
            && middleware.content.contains("pub async fn track_metrics"),
        "worker middleware must provide a metrics_middleware::track_metrics point"
    );

    // Monolith topology stays root-only.
    let (mock2, config2, tera2, output_dir2) = workers_scaffold_test_setup();
    let project2 = codegraph::generate::ProjectConfig {
        deployment: codegraph::generate::DeploymentConfig {
            deployment_topology: codegraph::profile::DeploymentTopology::Monolith,
        },
        identity: codegraph::generate::IdentityConfig {
            hooks_api_crate: "hr_hooks_api".to_string(),
            ..Default::default()
        },
        ..Default::default()
    };
    let (report2, _output_dir2) = run_routing_generators_with_parts(
        mock2,
        config2,
        tera2,
        output_dir2,
        codegraph::profile::DeploymentTopology::Monolith,
        project2,
    )
    .await;
    assert!(
        !report2
            .files
            .iter()
            .any(|f| f.path.to_string_lossy().contains("/workers/")),
        "monolith topology must not produce a workers/ directory"
    );
    assert!(
        report2
            .files
            .iter()
            .any(|f| f.path.to_string_lossy().ends_with("src/api/meta.rs"))
    );
    assert!(
        !report2
            .files
            .iter()
            .any(|f| f.path.to_string_lossy().ends_with("src/hooks/mod.rs"))
    );
    assert!(
        !report2
            .files
            .iter()
            .any(|f| f.path.to_string_lossy().ends_with("hooks/mod.rs"))
    );
}

/// Lexically normalize an absolute path (collapse `.`/`..`) into its ordered
/// normal components, for comparing generated relative paths against a base.
fn normalize_abs(path: &Path) -> Vec<std::ffi::OsString> {
    use std::path::Component;
    let mut out: Vec<std::ffi::OsString> = Vec::new();
    for c in path.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(seg) => out.push(seg.to_os_string()),
            Component::RootDir => out.clear(),
            Component::CurDir => {}
            Component::Prefix(_) => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_abs_collapses_dotdot() {
        assert_eq!(
            normalize_abs(Path::new("/a/b/c/../../d")),
            vec![std::ffi::OsString::from("a"), std::ffi::OsString::from("d")]
        );
    }
}

/// Workers topology + extension points (issue #503): the dispatcher module
/// is emitted INTO each covered worker crate, the worker lib declares it,
/// the worker AppState constructs it from the provider handle, and the
/// worker command module carries the notify call sites for covered
/// entities. Flag-off parity is pinned by the policy_rls canary.
#[tokio::test]
async fn workers_topology_emits_integration_dispatcher_per_worker() {
    let (mock, config, tera, output_dir) = workers_scaffold_test_setup();
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

    let project = codegraph::generate::ProjectConfig {
        deployment: codegraph::generate::DeploymentConfig {
            deployment_topology: codegraph::profile::DeploymentTopology::Workers,
        },
        integration: codegraph::generate::IntegrationFlags {
            has_extension_points: true,
            ..Default::default()
        },
        ..Default::default()
    };

    let plan = BuildPlan {
        entity_form: false,
        ifml_tanstack_forms: false,
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
            "scaffold".to_string(),
            "integration_dispatch".to_string(),
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
        persistence_provider: codegraph::profile::PersistenceProvider::SeaOrm,
        dto_key_casing: "snake".to_string(),
        deployment_topology: codegraph::profile::DeploymentTopology::Workers,
        dependency_strategy: codegraph::profile::DependencyStrategy::Rev,
        namespace_layout: false,
        ux_rules: false,
        expr_ir: false,
        ifml_e2e_auth: false,
        ifml_remote_functions: false,
        entity_remote_functions: false,
        testid_module: false,
        structural_gates: false,
        public_operations_rls: false,
        features: toml::Table::new(),
    };

    let report =
        codegraph::generate::run_generators_with_opts(codegraph::generate::GeneratorOpts {
            integration_manifests: None,
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

    let find = |needle: &str| {
        report
            .files
            .iter()
            .find(|f| f.path.to_string_lossy().ends_with(needle))
            .unwrap_or_else(|| panic!("missing generated file: {needle}"))
    };

    // The dispatcher module lands INSIDE the covered worker crate...
    let dispatcher = find("workers/compensation/src/integration_dispatch.rs");
    assert!(
        dispatcher
            .content
            .contains("pub struct IntegrationDispatcher"),
        "worker dispatcher module must carry the dispatcher:\n{}",
        dispatcher.content
    );
    // ...the worker lib declares it...
    assert!(
        find("workers/compensation/src/lib.rs")
            .content
            .contains("pub mod integration_dispatch;"),
        "worker lib must declare the integration_dispatch module"
    );
    // ...the worker AppState constructs it from the provider handle...
    let app_state = find("workers/compensation/src/app_state.rs");
    assert!(
        app_state
            .content
            .contains("IntegrationDispatcher::new(db.clone())"),
        "worker AppState must construct the dispatcher:\n{}",
        app_state.content
    );
    // ...and the worker command module fires it for the covered entity.
    let command = find("workers/compensation/src/domain/compensation/pay_run/command.rs");
    assert!(
        command
            .content
            .contains("dispatcher.notify(organization_id, \"compensation.pay_run\", \"created\""),
        "worker command module must carry the notify call site:\n{}",
        command.content
    );
    // ...and the worker main declares the module: the bin target compiles
    // the domain command modules, which reference `crate::integration_dispatch`
    // (issue #536 — without this the wasm bin E0433s while the lib compiles).
    assert!(
        find("workers/compensation/src/main.rs")
            .content
            .contains("mod integration_dispatch;"),
        "worker main must declare the integration_dispatch module:\n{}",
        find("workers/compensation/src/main.rs").content
    );
}

/// Workers topology + extension points, MULTI-domain coverage (issue #553):
/// the dispatcher surface is per-DOMAIN, not project-wide. The fixture has
/// two domains — `compensation` (push-covered via `compensation.pay_run`)
/// and `common` (uncovered) — so the uncovered worker's module tree must be
/// consistent: no orphan `mod integration_dispatch;` declarations, no
/// dispatcher construction, and no command-plane type references to the
/// module that was never emitted. Before the fix the scaffold gated on the
/// PROJECT-level `has_extension_points` flag, so `common`'s worker declared
/// the module (E0583) and constructed a dispatcher (E0433) for a file the
/// generator never wrote.
#[tokio::test]
async fn uncovered_domain_worker_has_no_dispatch_surface() {
    let (mock, config, tera, output_dir) = workers_scaffold_test_setup();
    let domain_types_tmp = tempfile::TempDir::new().unwrap();
    let hooks_tmp = tempfile::TempDir::new().unwrap();

    // Only compensation is push-covered — common carries no covered entity.
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

    let project = codegraph::generate::ProjectConfig {
        deployment: codegraph::generate::DeploymentConfig {
            deployment_topology: codegraph::profile::DeploymentTopology::Workers,
        },
        integration: codegraph::generate::IntegrationFlags {
            has_extension_points: true,
            ..Default::default()
        },
        ..Default::default()
    };

    let plan = BuildPlan {
        entity_form: false,
        ifml_tanstack_forms: false,
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
            "scaffold".to_string(),
            "integration_dispatch".to_string(),
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
        persistence_provider: codegraph::profile::PersistenceProvider::SeaOrm,
        dto_key_casing: "snake".to_string(),
        deployment_topology: codegraph::profile::DeploymentTopology::Workers,
        dependency_strategy: codegraph::profile::DependencyStrategy::Rev,
        namespace_layout: false,
        ux_rules: false,
        expr_ir: false,
        ifml_e2e_auth: false,
        ifml_remote_functions: false,
        entity_remote_functions: false,
        testid_module: false,
        structural_gates: false,
        public_operations_rls: false,
        features: toml::Table::new(),
    };

    let report =
        codegraph::generate::run_generators_with_opts(codegraph::generate::GeneratorOpts {
            integration_manifests: None,
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

    let find = |needle: &str| {
        report
            .files
            .iter()
            .find(|f| f.path.to_string_lossy().ends_with(needle))
            .unwrap_or_else(|| panic!("missing generated file: {needle}"))
    };
    let find_opt = |needle: &str| {
        report
            .files
            .iter()
            .find(|f| f.path.to_string_lossy().ends_with(needle))
            .map(|f| f.content.clone())
    };

    // The uncovered worker carries NO dispatcher module at all.
    assert!(
        find_opt("workers/common/src/integration_dispatch.rs").is_none(),
        "uncovered worker must not receive the integration_dispatch module"
    );

    // No orphan module declarations: the lib and bin must not reference the
    // module that was never emitted.
    let lib = find("workers/common/src/lib.rs");
    assert!(
        !lib.content.contains("integration_dispatch"),
        "uncovered worker lib must not declare the integration_dispatch module:\n{}",
        lib.content
    );
    let main = find("workers/common/src/main.rs");
    assert!(
        !main.content.contains("integration_dispatch"),
        "uncovered worker main must not declare the integration_dispatch module:\n{}",
        main.content
    );

    // No dispatcher construction in the AppState.
    let app_state = find("workers/common/src/app_state.rs");
    assert!(
        !app_state.content.contains("integration_dispatcher"),
        "uncovered worker AppState must not construct the dispatcher:\n{}",
        app_state.content
    );

    // The command plane must not type-reference the absent module.
    let command = find("workers/common/src/domain/common/code/command.rs");
    assert!(
        !command.content.contains("integration_dispatch"),
        "uncovered worker command module must not reference the dispatcher:\n{}",
        command.content
    );

    // Contrast: the covered domain keeps its full surface (also pinned by
    // workers_topology_emits_integration_dispatcher_per_worker).
    assert!(
        find_opt("workers/compensation/src/integration_dispatch.rs").is_some(),
        "covered worker must still receive the integration_dispatch module"
    );
}

/// Workers topology + extension points (issue #521): the integration
/// catalog/installations API is MOUNTED, not just emitted. The gateway
/// worker hosts the platform-level surface: the cornucopia catalog handlers
/// + router + typed config structs compile INSIDE the gateway crate (workers
/// topology compiles no root `src/` tree), the gateway router merges the
/// catalog router behind the shared worker auth middleware, and the
/// Cargo/wrangler graphs carry the DB + auth surface (PLATFORM_DB hyperdrive
/// binding, monolith-parity auth vars).
#[tokio::test]
async fn workers_topology_mounts_integration_catalog_router() {
    let (mock, config, tera, output_dir) = workers_scaffold_test_setup();
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

    let project = codegraph::generate::ProjectConfig {
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
        ..Default::default()
    };

    let plan = BuildPlan {
        entity_form: false,
        ifml_tanstack_forms: false,
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
            "scaffold".to_string(),
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
        // Workers topology mandates the cornucopia persistence provider
        // (BuildPlan validation) — the gateway's catalog surface is the
        // cornucopia handler variant.
        persistence_provider: codegraph::profile::PersistenceProvider::Cornucopia,
        dto_key_casing: "snake".to_string(),
        deployment_topology: codegraph::profile::DeploymentTopology::Workers,
        dependency_strategy: codegraph::profile::DependencyStrategy::Rev,
        namespace_layout: false,
        ux_rules: false,
        expr_ir: false,
        ifml_e2e_auth: false,
        ifml_remote_functions: false,
        entity_remote_functions: false,
        testid_module: false,
        structural_gates: false,
        public_operations_rls: false,
        features: toml::Table::new(),
    };

    let report =
        codegraph::generate::run_generators_with_opts(codegraph::generate::GeneratorOpts {
            integration_manifests: None,
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

    let find = |needle: &str| {
        report
            .files
            .iter()
            .find(|f| f.path.to_string_lossy().ends_with(needle))
            .unwrap_or_else(|| panic!("missing generated file: {needle}"))
    };

    // 1. The catalog trio lands INSIDE the gateway crate — the workers
    //    topology compiles no root `src/` tree, so root-only emission is
    //    dead output (the bug in issue #521).
    let router = find("workers/gateway/src/integration_router.rs");
    assert!(
        router.content.contains("pub fn integration_router()")
            && router.content.contains("/integrations/installations"),
        "gateway integration router must carry the installations routes:\n{}",
        router.content
    );
    let handler = find("workers/gateway/src/integration_catalog.rs");
    assert!(
        handler.content.contains("pub async fn install_integration"),
        "gateway catalog handlers must carry the installation CRUD:\n{}",
        handler.content
    );
    // The gateway is a cornucopia worker: raw SQL rides the per-request
    // GenericClient (DbClient), never SeaORM.
    assert!(
        handler.content.contains("GenericClient"),
        "gateway catalog handlers must be the cornucopia variant:\n{}",
        handler.content
    );
    assert!(
        !handler.content.contains("query_all_raw"),
        "gateway catalog handlers must not use the SeaORM monolith surface:\n{}",
        handler.content
    );
    // RLS scoping rides the connection (platform_integrations policies key
    // on get_current_org_id()), mirroring the worker webhook API.
    assert!(
        handler.content.contains("set_config('app.organization_id'"),
        "gateway catalog handlers must scope the connection for RLS:\n{}",
        handler.content
    );
    let config_rs = find("workers/gateway/src/integration_config.rs");
    assert!(
        config_rs.content.contains("pub struct PayrollSyncConfig"),
        "gateway integration config must carry the typed point structs:\n{}",
        config_rs.content
    );

    // 2. The compile closure: AppState (client source), the shared worker
    //    auth middleware, the error type, and the cornucopia client
    //    plumbing — all present in the gateway crate.
    let app_state = find("workers/gateway/src/app_state.rs");
    assert!(
        app_state
            .content
            .contains("pub client_source: Arc<dyn ClientSource>"),
        "gateway AppState must carry the cornucopia client source:\n{}",
        app_state.content
    );
    let middleware = find("workers/gateway/src/middleware.rs");
    assert!(
        middleware.content.contains("pub async fn auth_middleware"),
        "gateway must carry the shared worker auth middleware:\n{}",
        middleware.content
    );
    find("workers/gateway/src/error.rs");
    let db_client = find("workers/gateway/src/db_client.rs");
    assert!(
        db_client.content.contains("pub trait ClientSource"),
        "gateway must carry the cornucopia client plumbing:\n{}",
        db_client.content
    );

    // 3. The gateway lib declares the modules; the native bin declares its
    //    own copies (worker-crate convention).
    let lib = find("workers/gateway/src/lib.rs");
    for module in [
        "app_state",
        "db_client",
        "error",
        "integration_catalog",
        "integration_config",
        "integration_router",
        "middleware",
    ] {
        assert!(
            lib.content.contains(&format!("pub mod {module};")),
            "gateway lib must declare `pub mod {module};`:\n{}",
            lib.content
        );
    }
    let gateway_main = find("workers/gateway/src/main.rs");
    for module in ["app_state", "db_client", "integration_router", "middleware"] {
        assert!(
            gateway_main.content.contains(&format!("mod {module};")),
            "gateway main must declare `mod {module};`:\n{}",
            gateway_main.content
        );
    }

    // 4. The gateway router MERGES the catalog router behind the auth
    //    middleware — wasm fetch and the native dev twin both mount it.
    let worker = find("workers/gateway/src/worker.rs");
    assert!(
        worker
            .content
            .contains("crate::integration_router::integration_router()"),
        "gateway wasm router must mount the catalog router:\n{}",
        worker.content
    );
    assert!(
        worker
            .content
            .contains("integration_state.clone(), crate::middleware::auth_middleware"),
        "gateway catalog routes must sit behind the auth middleware:\n{}",
        worker.content
    );
    assert!(
        gateway_main
            .content
            .contains("crate::integration_router::integration_router()"),
        "gateway native router must mount the catalog router:\n{}",
        gateway_main.content
    );

    // 5. The Cargo graphs compile it: the gateway crate carries the
    //    cornucopia/auth deps (workspace entries exist for the domain
    //    workers already).
    let cargo = find("workers/gateway/Cargo.toml");
    for dep in [
        "cornucopia-queries",
        "tokio-postgres",
        "jsonwebtoken",
        "async-trait",
    ] {
        assert!(
            cargo.content.contains(dep),
            "gateway Cargo.toml must carry `{dep}`:\n{}",
            cargo.content
        );
    }
    let workspace = find("workers/Cargo.toml");
    assert!(
        workspace.content.contains("tokio-postgres"),
        "workers workspace manifest must carry the cornucopia deps:\n{}",
        workspace.content
    );

    // 6. wrangler carries the platform DB binding + the auth vars.
    let wrangler = find("workers/gateway/wrangler.toml");
    assert!(
        wrangler.content.contains("binding = \"PLATFORM_DB\""),
        "gateway wrangler must declare the PLATFORM_DB hyperdrive binding:\n{}",
        wrangler.content
    );
    assert!(
        wrangler.content.contains("SUPABASE_JWT_SECRET"),
        "gateway wrangler must declare the auth vars:\n{}",
        wrangler.content
    );
}

/// Flag-off byte-identity (issue #521): without `--extension-points` the
/// workers gateway stays EXACTLY the thin proxy it was — no catalog files,
/// no platform modules, no DB/auth deps, no hyperdrive binding. Same
/// fixture as the mount test (cornucopia + workers topology), flag off.
#[tokio::test]
async fn workers_topology_without_extension_points_keeps_gateway_clean() {
    let (mock, config, tera, output_dir) = workers_scaffold_test_setup();
    let domain_types_tmp = tempfile::TempDir::new().unwrap();
    let hooks_tmp = tempfile::TempDir::new().unwrap();

    let project = codegraph::generate::ProjectConfig {
        database: codegraph::generate::DatabaseConfig {
            database_target: codegraph::generate::db::dialect::DatabaseTarget::Postgres,
            persistence_provider: codegraph::profile::PersistenceProvider::Cornucopia,
        },
        deployment: codegraph::generate::DeploymentConfig {
            deployment_topology: codegraph::profile::DeploymentTopology::Workers,
        },
        ..Default::default()
    };

    let plan = BuildPlan {
        entity_form: false,
        ifml_tanstack_forms: false,
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
            "scaffold".to_string(),
            // The integration generators are still PLANned — the presence
            // gate is the parsed --extension-points config (registry side),
            // so a flag-off run must stay byte-identical even with them
            // listed.
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
        ifml_remote_functions: false,
        entity_remote_functions: false,
        testid_module: false,
        structural_gates: false,
        public_operations_rls: false,
        features: toml::Table::new(),
    };

    let report =
        codegraph::generate::run_generators_with_opts(codegraph::generate::GeneratorOpts {
            integration_manifests: None,
            db: &mock,
            config: &config,
            output_dir: output_dir.path(),
            tera: &tera,
            ui_overrides: &Default::default(),
            ui_domains: &Default::default(),
            schema_base_dir: Path::new(""),
            domain_types_base: Some(domain_types_tmp.path()),
            hooks_base: Some(hooks_tmp.path()),
            ext_points: None,
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

    // No integration file lands anywhere in the tree — not at the root (the
    // pre-#521 dead output), not in the gateway crate.
    let offenders: Vec<String> = report
        .files
        .iter()
        .filter(|f| {
            let path = f.path.to_string_lossy();
            path.ends_with("integration_catalog.rs")
                || path.ends_with("integration_router.rs")
                || path.ends_with("integration_config.rs")
                || path.ends_with("integration_dispatch.rs")
        })
        .map(|f| f.path.to_string_lossy().to_string())
        .collect();
    assert!(
        offenders.is_empty(),
        "flag-off run must emit no integration plane files, found: {offenders:?}"
    );

    let find = |needle: &str| {
        report
            .files
            .iter()
            .find(|f| f.path.to_string_lossy().ends_with(needle))
            .unwrap_or_else(|| panic!("missing generated file: {needle}"))
    };

    // The gateway stays the thin proxy: no platform modules, no mount, no
    // DB/auth deps, no hyperdrive binding.
    for forbidden in [
        "workers/gateway/src/app_state.rs",
        "workers/gateway/src/db_client.rs",
        "workers/gateway/src/middleware.rs",
    ] {
        assert!(
            !report
                .files
                .iter()
                .any(|f| f.path.to_string_lossy().ends_with(forbidden)),
            "flag-off gateway must not emit {forbidden}"
        );
    }
    let lib = find("workers/gateway/src/lib.rs");
    assert!(
        !lib.content.contains("integration_") && !lib.content.contains("mod middleware;"),
        "flag-off gateway lib must stay the thin proxy:\n{}",
        lib.content
    );
    let worker = find("workers/gateway/src/worker.rs");
    assert!(
        !worker.content.contains("integration_router"),
        "flag-off gateway router must not mount the catalog:\n{}",
        worker.content
    );
    let gateway_main = find("workers/gateway/src/main.rs");
    assert!(
        !gateway_main.content.contains("integration_router"),
        "flag-off gateway main must not mount the catalog:\n{}",
        gateway_main.content
    );
    let cargo = find("workers/gateway/Cargo.toml");
    assert!(
        !cargo.content.contains("cornucopia-queries") && !cargo.content.contains("jsonwebtoken"),
        "flag-off gateway Cargo must stay dependency-lean:\n{}",
        cargo.content
    );
    let wrangler = find("workers/gateway/wrangler.toml");
    assert!(
        !wrangler.content.contains("PLATFORM_DB") && !wrangler.content.contains("hyperdrive"),
        "flag-off gateway wrangler must stay proxy-only:\n{}",
        wrangler.content
    );
}

/// Monolith topology + extension points (issue #521, "monolith mounting
/// likewise"): the generated catalog surface compiles (main declares the
/// modules — including `integration_dispatch`, whose absence was a pre-#521
/// E0433 with extension points on) and `server` merges the catalog router
/// behind the monolith auth middleware. Flag-off keeps the stub-only shape.
#[tokio::test]
async fn monolith_declares_and_mounts_integration_catalog() {
    let (mock, config, tera, output_dir) = workers_scaffold_test_setup();
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

    let project = codegraph::generate::ProjectConfig {
        deployment: codegraph::generate::DeploymentConfig {
            deployment_topology: codegraph::profile::DeploymentTopology::Monolith,
        },
        integration: codegraph::generate::IntegrationFlags {
            has_extension_points: true,
            ..Default::default()
        },
        ..Default::default()
    };

    let plan = BuildPlan {
        entity_form: false,
        ifml_tanstack_forms: false,
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
            "scaffold".to_string(),
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
        persistence_provider: codegraph::profile::PersistenceProvider::SeaOrm,
        dto_key_casing: "snake".to_string(),
        deployment_topology: codegraph::profile::DeploymentTopology::Monolith,
        dependency_strategy: codegraph::profile::DependencyStrategy::Rev,
        namespace_layout: false,
        ux_rules: false,
        expr_ir: false,
        ifml_e2e_auth: false,
        ifml_remote_functions: false,
        entity_remote_functions: false,
        testid_module: false,
        structural_gates: false,
        public_operations_rls: false,
        features: toml::Table::new(),
    };

    let report =
        codegraph::generate::run_generators_with_opts(codegraph::generate::GeneratorOpts {
            integration_manifests: None,
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

    let find = |needle: &str| {
        report
            .files
            .iter()
            .find(|f| f.path.to_string_lossy().ends_with(needle))
            .unwrap_or_else(|| panic!("missing generated file: {needle}"))
    };

    // The catalog plane lands at the monolith root (SeaORM handler variant).
    let catalog = find("src/integration_catalog.rs");
    assert!(
        catalog.content.contains("query_all_raw"),
        "monolith catalog handlers stay the SeaORM variant:\n{}",
        catalog.content
    );
    find("src/integration_router.rs");
    find("src/integration_config.rs");

    // main declares every integration module (both the admin-CLI and the
    // plain binary branches render the same block).
    let main = find("src/main.rs");
    for module in [
        "integration_dispatch",
        "integration_catalog",
        "integration_router",
        "integration_config",
    ] {
        assert!(
            main.content.contains(&format!("mod {module};")),
            "monolith main must declare `mod {module};`:\n{}",
            main.content
        );
    }

    // server merges the generated router behind the monolith auth
    // middleware (full paths inside the router forbid nesting under
    // /api/{version}).
    let server = find("src/server.rs");
    assert!(
        server
            .content
            .contains("crate::integration_router::integration_router()"),
        "monolith server must mount the catalog router:\n{}",
        server.content
    );
    assert!(
        server
            .content
            .contains("from_fn_with_state(\n                    state.clone(),\n                    crate::middleware::auth_middleware,\n                )"),
        "monolith catalog routes must sit behind the auth middleware:\n{}",
        server.content
    );

    // lib re-exports the modules for integration tests.
    let lib = find("src/lib.rs");
    assert!(
        lib.content.contains("pub mod integration_catalog;")
            && lib.content.contains("pub mod integration_router;")
            && lib.content.contains("pub mod integration_config;"),
        "monolith lib must re-export the catalog modules:\n{}",
        lib.content
    );

    // The inert status stub stays untouched.
    assert!(
        server
            .content
            .contains("crate::integrations::get_integrations"),
        "the /integrations status stub route must remain:\n{}",
        server.content
    );

    // Flag-off pin: without extension points, none of the above.
    let (mock2, config2, tera2, output_dir2) = workers_scaffold_test_setup();
    let project2 = codegraph::generate::ProjectConfig {
        deployment: codegraph::generate::DeploymentConfig {
            deployment_topology: codegraph::profile::DeploymentTopology::Monolith,
        },
        ..Default::default()
    };
    let report2 =
        codegraph::generate::run_generators_with_opts(codegraph::generate::GeneratorOpts {
            integration_manifests: None,
            db: &mock2,
            config: &config2,
            output_dir: output_dir2.path(),
            tera: &tera2,
            ui_overrides: &Default::default(),
            ui_domains: &Default::default(),
            schema_base_dir: Path::new(""),
            domain_types_base: Some(domain_types_tmp.path()),
            hooks_base: Some(hooks_tmp.path()),
            ext_points: None,
            seed_config: None,
            build_plan: Some(&plan),
            ifml_frameworks: vec![],
            ifml_components: None,
            ux_rules: None,
            project_config: Some(&project2),
            emdash_plugins: None,
            domain_config_dir: None,
        })
        .await
        .unwrap();
    let find2 = |needle: &str| {
        report2
            .files
            .iter()
            .find(|f| f.path.to_string_lossy().ends_with(needle))
            .unwrap_or_else(|| panic!("missing generated file: {needle}"))
    };
    assert!(
        !report2.files.iter().any(|f| f
            .path
            .to_string_lossy()
            .ends_with("src/integration_catalog.rs")),
        "flag-off monolith must emit no catalog surface"
    );
    let main2 = find2("src/main.rs");
    assert!(
        !main2.content.contains("mod integration_"),
        "flag-off monolith main must declare no integration modules:\n{}",
        main2.content
    );
    let server2 = find2("src/server.rs");
    assert!(
        !server2.content.contains("integration_router"),
        "flag-off monolith server must not mount the catalog:\n{}",
        server2.content
    );
}
