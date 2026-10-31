//! Shared review-fixture regeneration (used by `review_api_suite` and the
//! #574 S2 `entity_codegen_gate`) — ONE recipe so the gates can never
//! drift apart: the full default-profile pipeline over
//! `tests/fixtures/{schemas,classifier.toml,domains.toml}` written into
//! `crates/review/generated-candidate/`, minus the gRPC (protoc) and ops
//! (would overwrite the checked-in manifest/testkit) generators.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub fn manifest_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

pub fn fixture_dir() -> PathBuf {
    manifest_dir()
        .parent()
        .unwrap()
        .join("review")
        .join("generated-candidate")
}

pub fn ensure_testkit_workspace_member(fixture_dir: &Path) {
    let cargo = fixture_dir.join("Cargo.toml");
    let Ok(text) = std::fs::read_to_string(&cargo) else {
        return;
    };
    if text.contains("\"testkit\"") {
        return;
    }
    let patched = text.replace(
        "members = [\".\", \"cli\", \"migration\"]",
        "members = [\".\", \"cli\", \"migration\", \"testkit\"]",
    );
    if patched != text {
        let _ = std::fs::write(&cargo, patched);
    }
}

pub fn ensure_testkit_path_dep(fixture_dir: &Path) {
    let cargo = fixture_dir.join("testkit").join("Cargo.toml");
    let Ok(text) = std::fs::read_to_string(&cargo) else {
        return;
    };
    let patched = text
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("codegraph-ops") {
                "codegraph-ops = { path = \"../../../codegraph-ops\" }".to_string()
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if patched != text {
        let _ = std::fs::write(&cargo, patched);
    }
}

/// Regenerate the fixture app in place from the current templates.
///
/// - `output_dir` is the checked-in fixture dir (the fixture tracks the
///   branch, like `grafeo_candidate_inspect_output`);
/// - the domain-types crate is generated INSIDE the app output so the
///   scaffolded `domain-types = { path = "domain-types" }` dependency
///   resolves;
/// - the gRPC generators (need protoc) and the `ops` generator (would
///   overwrite the checked-in `codegraph-ops.toml` + `testkit/`) are stripped
///   from the plan.
pub async fn regenerate_fixture() -> Result<(), String> {
    use codegraph::generate::template_engine::create_tera;
    use codegraph::generate::{
        DtoKeyCasing, GeneratorOpts, ProjectConfig, compute_generation_order,
        run_generators_with_opts,
    };
    use codegraph::profile::{BuildPlan, CapabilityRegistry, load_and_resolve_profile};
    use codegraph_classifier::config::parse_classifier_config;
    use codegraph_config::UiOverrideConfig;
    use codegraph_grafeo::GrafeoEngine;

    let mdir = manifest_dir();
    let workspace_root = mdir.parent().unwrap().parent().unwrap();
    let output_dir = fixture_dir();

    let config =
        codegraph_config::config::parse_domain_config(&mdir.join("tests/fixtures/domains.toml"))
            .map_err(|e| format!("parse domains.toml: {e}"))?;
    let classifier = parse_classifier_config(&mdir.join("tests/fixtures/classifier.toml"))
        .map_err(|e| format!("parse classifier.toml: {e}"))?;
    let entity_names: HashSet<String> = config
        .domains
        .values()
        .flat_map(|d| d.entities.iter().cloned())
        .collect();

    let engine = GrafeoEngine::in_memory().map_err(|e| format!("grafeo: {e}"))?;
    codegraph::ingest::async_ingest::ingest_schemas(
        &engine,
        Path::new("tests/fixtures/schemas"),
        &classifier,
        &entity_names,
        &UiOverrideConfig::default(),
        &config.defaults.type_suffix,
    )
    .await
    .map_err(|e| format!("ingest fixture schemas: {e}"))?;

    // The generated app must compile without touching the network: path deps
    // into this workspace, domain-types inside the app output.
    let type_contracts_path = workspace_root.join("crates/codegraph-type-contracts");
    let workflow_path = workspace_root.join("crates/codegraph-workflow");

    let registry = CapabilityRegistry::new();
    let mut resolved =
        load_and_resolve_profile(&workspace_root.join("profiles.toml"), "default", None)
            .map_err(|e| format!("resolve profile: {e}"))?;
    for section in resolved.sections.values_mut() {
        section
            .generators
            .retain(|g| !g.starts_with("grpc_") && g != "ops");
    }
    let plan =
        BuildPlan::from_profile(&resolved, &registry).map_err(|e| format!("build plan: {e}"))?;

    let project_config = ProjectConfig {
        identity: codegraph::generate::IdentityConfig {
            app_name: "app".into(),
            domain_types_crate: "domain_types".into(),
            generator_name: "codegraph-review-gate".into(),
            ..Default::default()
        },
        paths: codegraph::generate::PathsConfig {
            type_contracts_base: type_contracts_path.to_string_lossy().to_string(),
            codegraph_workflow_base: workflow_path.to_string_lossy().to_string(),
            domain_types_base: "domain-types".into(),
            ..Default::default()
        },
        cargo: codegraph::generate::CargoConfig {
            extra_dependencies: format!(
                "codegraph-workflow = {{ path = \"{}\" }}\n\
                 codegraph-type-contracts = {{ path = \"{}\" }}",
                workflow_path.display(),
                type_contracts_path.display()
            ),
            ..Default::default()
        },
        codegen: codegraph::generate::CodegenConfig {
            // Thread the plan's codegen flags (the driver does the same):
            // without this the fixture regenerates with every codegen flag
            // defaulted OFF and the gate validates a plane the shipped
            // profiles never produce.
            dto_key_casing: DtoKeyCasing::from_config(&plan.dto_key_casing),
            namespace_layout: plan.namespace_layout,
            expr_ir: plan.expr_ir,
            ifml_e2e_auth: plan.ifml_e2e_auth,
            ifml_remote_functions: plan.ifml_remote_functions,
            entity_remote_functions: plan.entity_remote_functions,
            testid_module: plan.testid_module,
            structural_gates: plan.structural_gates,
            types_import_prefix: config.defaults.types_import_prefix.clone(),
        },
        ..Default::default()
    };

    let domain_types_dir = output_dir.join("domain-types");
    let hooks_tmp = tempfile::tempdir().map_err(|e| format!("hooks tempdir: {e}"))?;

    // The generation order is needed twice: by the pipeline itself and to
    // assert the graph actually produced entities (an empty graph would
    // "regenerate" nothing and let a stale fixture pass).
    let order = compute_generation_order(&engine, &config)
        .await
        .map_err(|e| format!("generation order: {e}"))?;
    if order.is_empty() {
        return Err("generation order is empty — fixture schemas failed to ingest".into());
    }

    let tera = create_tera(Path::new("unused")).map_err(|e| format!("tera: {e}"))?;
    let report = run_generators_with_opts(GeneratorOpts {
        integration_manifests: None,
        db: &engine,
        config: &config,
        output_dir: &output_dir,
        tera: &tera,
        ui_overrides: &UiOverrideConfig::default(),
        ui_domains: &codegraph_config::UiDomainConfig::default(),
        schema_base_dir: Path::new(""),
        seed_config: None,
        domain_types_base: Some(&domain_types_dir),
        hooks_base: Some(hooks_tmp.path()),
        ext_points: None,
        build_plan: Some(&plan),
        ifml_frameworks: vec![],
        ifml_components: None,
        ux_rules: None,
        project_config: Some(&project_config),
        emdash_plugins: None,
        domain_config_dir: None,
    })
    .await
    .map_err(|e| format!("regenerate fixture: {e}"))?;

    if report.has_errors() {
        let detail = report
            .errors
            .iter()
            .map(|e| format!("{}/{}: {}", e.entity, e.generator, e.source))
            .take(10)
            .collect::<Vec<_>>()
            .join("\n");
        return Err(format!("fixture regeneration reported errors:\n{detail}"));
    }

    ensure_testkit_workspace_member(&output_dir);
    ensure_testkit_path_dep(&output_dir);
    Ok(())
}
