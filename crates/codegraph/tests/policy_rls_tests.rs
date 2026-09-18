//! Policy-driven RLS generator tests (issue #219).
//!
//! Three layers, mirroring the grpc_snapshot_tests / profile_smoke_tests /
//! determinism_tests patterns:
//!
//! 1. Flag-off byte-identity: with `rls_from_policy` absent from the build
//!    plan, the FULL fixture pipeline output is byte-identical to the
//!    pre-feature baseline (snapshot captured from `master` before the
//!    generator existed). The actor policy IS present in the graph — only the
//!    flag keeps the generator off.
//! 2. Profile validation: the `policy_rls` capability is registered,
//!    requires the `rls_from_policy` feature, and is skipped on plan-less
//!    runs (like the gRPC backend-flag generators).
//! 3. Snapshot tests for the emitted SQL live in
//!    `policy_rls_snapshot_tests.rs`; expr→SQL lowering unit tests live with
//!    the lowering module (`db/expr_sql.rs`).

#[path = "test_framework/mod.rs"]
mod test_framework;

use std::collections::BTreeMap;
use std::path::Path;

use codegraph::generate::{run_generators_with_opts, GeneratorOpts, ProjectConfig};
use codegraph_core::traits::{GraphIngestor, GraphQuerier};
use codegraph_core::types::{
    ActorNode, ActorPolicyModel, ActorPolicyNode, CapabilityNode, GrantEdge,
};
use codegraph_grafeo::GrafeoEngine;

fn profiles_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .unwrap()
        .join("profiles.toml")
}

/// The fixture actor policy ingested into the engine for every test here:
/// two human actors, one agent (extends a human), two capabilities bound to
/// the `CandidateType` class (one via the rexlang-qualified
/// `recruiting::CandidateType` form, one bare), permit + forbid effects, and
/// a `when` expression on one permit.
fn fixture_policy() -> ActorPolicyModel {
    let actors = vec![
        ActorNode {
            name: "Recruiter".into(),
            kind: Some("human".into()),
            extends: None,
            block: Some("hiring".into()),
        },
        ActorNode {
            name: "Auditor".into(),
            kind: Some("human".into()),
            extends: Some("Recruiter".into()),
            block: Some("hiring".into()),
        },
        ActorNode {
            name: "Screener".into(),
            kind: Some("agent".into()),
            extends: Some("Recruiter".into()),
            block: Some("hiring".into()),
        },
    ];
    let capabilities = vec![
        CapabilityNode {
            name: "ReviewCandidate".into(),
            class: "recruiting::CandidateType".into(),
            block: Some("hiring".into()),
        },
        CapabilityNode {
            name: "ArchiveCandidate".into(),
            class: "CandidateType".into(),
            block: Some("hiring".into()),
        },
    ];
    let grant = |actor: &str, capability: &str, effect: &str, when: Option<&str>| GrantEdge {
        actor: actor.into(),
        capability: capability.into(),
        effect: effect.into(),
        when: when.map(|w| w.into()),
        obligations: vec![],
    };
    let grants = vec![
        grant(
            "Recruiter",
            "ReviewCandidate",
            "permit",
            Some("status == \"active\""),
        ),
        grant("Auditor", "ReviewCandidate", "permit", None),
        grant("Auditor", "ArchiveCandidate", "forbid", None),
    ];
    ActorPolicyModel {
        actors,
        capabilities,
        grants,
        policy: ActorPolicyNode {
            blocks: vec!["hiring".into()],
            never_both: vec![],
            purposes: vec![],
            delegations: vec![],
        },
    }
}

/// The full fixture pipeline (mirrors `determinism_tests::run_generation`,
/// minus gRPC) with the fixture actor policy ingested into the graph.
async fn run_fixture_generation(output_dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let (engine, config) = {
        let config =
            codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml"))
                .unwrap();
        let classifier = codegraph_classifier::config::parse_classifier_config(Path::new(
            "tests/fixtures/classifier.toml",
        ))
        .unwrap();
        let entity_names: std::collections::HashSet<String> = config
            .domains
            .values()
            .flat_map(|d| d.entities.iter().cloned())
            .collect();
        let engine = GrafeoEngine::in_memory().unwrap();
        codegraph::ingest::async_ingest::ingest_schemas(
            &engine,
            Path::new("tests/fixtures/schemas"),
            &classifier,
            &entity_names,
            &codegraph_config::UiOverrideConfig::default(),
            &config.defaults.type_suffix,
        )
        .await
        .unwrap();
        engine.ingest_actor_policy(&fixture_policy()).await.unwrap();
        (engine, config)
    };

    // The policy must actually be in the graph for this test to prove
    // anything: flag-off (not absent policy data) is the contract.
    assert!(
        !engine.get_actors().await.unwrap().is_empty(),
        "fixture policy must be ingested"
    );

    let tera = codegraph::generate::template_engine::create_tera(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("templates"),
    )
    .unwrap();

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    let type_contracts_path = workspace_root
        .join("crates")
        .join("codegraph-type-contracts");
    let workflow_path = workspace_root.join("crates").join("codegraph-workflow");
    let profiles = profiles_path();

    let registry = codegraph::profile::CapabilityRegistry::new();
    let mut resolved =
        codegraph::profile::load_and_resolve_profile(&profiles, "default", None).unwrap();
    for section in resolved.sections.values_mut() {
        section.generators.retain(|g| !g.starts_with("grpc_"));
    }
    let plan = codegraph::profile::BuildPlan::from_profile(&resolved, &registry).unwrap();

    let domain_types_dir = output_dir.join("domain-types");
    let hooks_tmp = tempfile::TempDir::new().unwrap();

    let project_config = ProjectConfig {
        app_name: "test-app".into(),
        domain_types_crate: "domain_types".into(),
        generator_name: "codegraph-test".into(),
        type_contracts_base: type_contracts_path.to_string_lossy().to_string(),
        codegraph_workflow_base: workflow_path.to_string_lossy().to_string(),
        domain_types_base: "domain-types".into(),
        types_import_prefix: config.defaults.types_import_prefix.clone(),
        extra_dependencies: format!(
            "codegraph-workflow = {{ path = \"{}\" }}\n\
             codegraph-type-contracts = {{ path = \"{}\" }}",
            workflow_path.display(),
            type_contracts_path.display(),
        ),
        ..Default::default()
    };

    let report = run_generators_with_opts(GeneratorOpts {
        db: &engine,
        config: &config,
        output_dir,
        tera: &tera,
        ui_overrides: &Default::default(),
        ui_domains: &Default::default(),
        schema_base_dir: Path::new(""),
        seed_config: None,
        domain_types_base: Some(&domain_types_dir),
        hooks_base: Some(hooks_tmp.path()),
        ext_points: None,
        build_plan: Some(&plan),
        ifml_frameworks: vec![],
        ifml_components: None,
        project_config: Some(&project_config),
        emdash_plugins: None,
        domain_config_dir: None,
    })
    .await
    .unwrap();

    assert!(
        !report.has_errors(),
        "generation reported errors: {:?}",
        report.errors
    );

    collect_tree(output_dir)
}

/// Recursively collect `relative path -> bytes` for every file under `root`
/// (same shape as `determinism_tests::collect_tree`).
fn collect_tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    fn walk(dir: &Path, base: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                walk(&path, base, out);
            } else {
                let rel = path
                    .strip_prefix(base)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                let bytes = fs::read(&path).unwrap();
                out.insert(rel, bytes);
            }
        }
    }
    walk(root, root, &mut out);
    out
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

use std::fs;

/// FLAG-OFF BYTE-IDENTITY (pinned decision #4): with `rls_from_policy`
/// absent (the default profile does not enable it and does not list
/// `policy_rls`), the full generated tree is byte-identical to the
/// pre-feature baseline captured from `master` — even though the actor
/// policy IS present in the graph. Hashes (not raw bytes) keep the snapshot
/// reviewable; any byte change to any file changes exactly that file's hash.
#[test]
fn flag_off_full_output_matches_master_baseline() {
    let dir = tempfile::TempDir::new().unwrap();
    let tree = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(run_fixture_generation(dir.path()));

    let hashes: BTreeMap<String, String> = tree
        .iter()
        .map(|(path, bytes)| (path.clone(), sha256_hex(bytes)))
        .collect();

    let policy_rls: Vec<&String> = hashes.keys().filter(|p| p.contains("policy_rls")).collect();
    assert!(
        policy_rls.is_empty(),
        "flag-off run must not emit policy RLS files, found: {policy_rls:?}"
    );

    insta::assert_yaml_snapshot!("flag_off_full_output_hashes", hashes);
}

// ── Profile validation (profile_smoke_tests pattern) ────────────────────────

#[test]
fn policy_rls_capability_is_registered_and_flag_gated() {
    let registry = codegraph::profile::CapabilityRegistry::new();
    let cap = registry
        .get("policy_rls")
        .expect("policy_rls capability must be registered");
    assert_eq!(cap.features_required, vec!["rls_from_policy".to_string()]);
    assert!(
        registry.requires_build_plan("policy_rls"),
        "policy_rls must be skipped on plan-less runs (rls_from_policy is a backend flag)"
    );
    assert!(registry.requires_build_plan("grpc_scaffold"));
}

#[test]
fn build_plan_without_flag_excludes_policy_rls() {
    let registry = codegraph::profile::CapabilityRegistry::new();
    let resolved =
        codegraph::profile::load_and_resolve_profile(&profiles_path(), "default", None).unwrap();
    let plan = codegraph::profile::BuildPlan::from_profile(&resolved, &registry).unwrap();
    assert!(
        !plan.has_global_gen("policy_rls"),
        "default profile must not run policy_rls with the flag off"
    );
}

#[test]
fn build_plan_with_flag_includes_policy_rls() {
    let toml = r#"
[profiles.policy.meta]
name = "policy"
version = "1.0.0"
description = "synthetic profile"

[profiles.policy.features]
rls_from_policy = true

[profiles.policy.api]
generators = ["ddl", "policy_rls"]
output = "out/"
"#;
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("profiles.toml");
    fs::write(&path, toml).unwrap();

    let registry = codegraph::profile::CapabilityRegistry::new();
    let resolved = codegraph::profile::load_and_resolve_profile(&path, "policy", None).unwrap();
    let plan = codegraph::profile::BuildPlan::from_profile(&resolved, &registry)
        .expect("profile listing policy_rls with rls_from_policy = true must validate");
    assert!(plan.has_global_gen("policy_rls"));
}

#[test]
fn build_plan_listing_policy_rls_without_flag_fails_validation() {
    let toml = r#"
[profiles.policy.meta]
name = "policy"
version = "1.0.0"
description = "synthetic profile"

[profiles.policy.features]

[profiles.policy.api]
generators = ["policy_rls"]
output = "out/"
"#;
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("profiles.toml");
    fs::write(&path, toml).unwrap();

    let registry = codegraph::profile::CapabilityRegistry::new();
    let resolved = codegraph::profile::load_and_resolve_profile(&path, "policy", None).unwrap();
    let err = codegraph::profile::BuildPlan::from_profile(&resolved, &registry)
        .expect_err("policy_rls without rls_from_policy must be a config error");
    let message = err.to_string();
    assert!(
        message.contains("rls_from_policy"),
        "error must name the missing feature, got: {message}"
    );
}
