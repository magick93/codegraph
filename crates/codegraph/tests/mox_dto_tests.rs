//! mox derived-feature DTO consumer tests (issues #218, #195).
//!
//! The consuming generator: a derived feature whose name matches an entity
//! field (matched from the graph by mox class name → schema title/entity
//! name, then field name) is marked computed/read-only in the response DTO
//! and omitted from the create/update DTOs.
//!
//! Presence-gating: with NO mox ingested the same pipeline produces
//! byte-identical DTOs — `pub candidate_id` stays everywhere and no
//! computed marker appears. Full-tree byte-identity is additionally pinned
//! by `policy_rls_tests::flag_off_full_output_hashes`.

use std::fs;
use std::path::{Path, PathBuf};

use codegraph_grafeo::GrafeoEngine;

/// Marks an existing entity field (`candidateId` from CandidateType.json) as
/// derived — the shape the DTO consumer keys on.
const DERIVED_FIELD_MOX: &str = r#"
package recruiting

class CandidateType {
    derived String candidateId {
        expr { "computed" }
    }
}
"#;

/// Writes a `.mox` file plus its vendored vocabulary snapshot (the snapshot
/// must exist on disk next to the model or vocabulary lowering errors).
fn write_mox_fixture(dir: &Path, name: &str, source: &str) -> PathBuf {
    const VOCAB_SNAPSHOT: &str = r#"{
  "vocabulary": "iso:4217",
  "version": "2024-01-01",
  "entries": [
    { "alpha3": "USD", "symbol": "$", "minorUnits": 2 }
  ]
}"#;
    let vocab_dir = dir.join("vocab");
    fs::create_dir_all(&vocab_dir).unwrap();
    fs::write(vocab_dir.join("iso-4217@2024-01-01.json"), VOCAB_SNAPSHOT).unwrap();
    let path = dir.join(name);
    fs::write(&path, source).unwrap();
    path
}

/// Ingest the fixture JSON schemas (schema entities must exist for class
/// name matching).
async fn ingest_fixture_schemas(
    engine: &GrafeoEngine,
    config: &codegraph_config::config::DomainConfig,
) {
    let classifier = codegraph_classifier::config::parse_classifier_config(Path::new(
        "tests/fixtures/classifier.toml",
    ))
    .unwrap();
    let entity_names: std::collections::HashSet<String> = config
        .domains
        .values()
        .flat_map(|d| d.entities.iter().cloned())
        .collect();
    codegraph::ingest::async_ingest::ingest_schemas(
        engine,
        Path::new("tests/fixtures/schemas"),
        &classifier,
        &entity_names,
        &codegraph_config::UiOverrideConfig::default(),
        &config.defaults.type_suffix,
    )
    .await
    .unwrap();
}

fn profiles_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .unwrap()
        .join("profiles.toml")
}

/// Run the full generator suite (minus gRPC) over the fixture graph and
/// return the three candidate domain-types DTO files.
async fn run_generation_and_read_dtos(
    engine: &GrafeoEngine,
    domain_types_dir: &Path,
    hooks_tmp: &Path,
    output_dir: &Path,
) -> (String, String, String) {
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

    let project_config = codegraph::generate::ProjectConfig {
        app_name: "test-app".into(),
        domain_types_crate: "domain_types".into(),
        generator_name: "codegraph-test".into(),
        type_contracts_base: type_contracts_path.to_string_lossy().to_string(),
        codegraph_workflow_base: workflow_path.to_string_lossy().to_string(),
        domain_types_base: "domain-types".into(),
        types_import_prefix: "crate::types".into(),
        ..Default::default()
    };

    let config =
        codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml"))
            .unwrap();

    let report =
        codegraph::generate::run_generators_with_opts(codegraph::generate::GeneratorOpts {
            db: engine,
            config: &config,
            output_dir,
            tera: &tera,
            ui_overrides: &Default::default(),
            ui_domains: &Default::default(),
            schema_base_dir: Path::new(""),
            seed_config: None,
            domain_types_base: Some(domain_types_dir),
            hooks_base: Some(hooks_tmp),
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
    assert!(!report.has_errors(), "generation reported errors");

    let dto_dir = domain_types_dir
        .join("src")
        .join("recruiting")
        .join("candidate");
    let response = fs::read_to_string(dto_dir.join("dto_response.rs"))
        .expect("dto_response.rs must be generated");
    let create =
        fs::read_to_string(dto_dir.join("dto_create.rs")).expect("dto_create.rs must be generated");
    let update =
        fs::read_to_string(dto_dir.join("dto_update.rs")).expect("dto_update.rs must be generated");
    (response, create, update)
}

#[tokio::test]
async fn mox_derived_field_is_readonly_in_response_and_absent_from_create_update() {
    let dir = tempfile::tempdir().unwrap();
    let output_dir = dir.path().join("out");
    let domain_types_dir = dir.path().join("domain-types");
    fs::create_dir_all(&output_dir).unwrap();

    let engine = GrafeoEngine::in_memory().unwrap();
    let config =
        codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml"))
            .unwrap();
    ingest_fixture_schemas(&engine, &config).await;

    let mox_dir = tempfile::tempdir().unwrap();
    let mox = write_mox_fixture(mox_dir.path(), "model.mox", DERIVED_FIELD_MOX);
    let stats = codegraph::ingest::mox_ingest::ingest_mox_files(
        &engine,
        &engine,
        &[mox],
        &config,
        &config.defaults.type_suffix,
    )
    .await
    .unwrap();
    assert_eq!(stats.derived_features, 1);
    assert_eq!(stats.skipped, 0);

    let hooks_tmp = tempfile::tempdir().unwrap();
    let (response, create, update) =
        run_generation_and_read_dtos(&engine, &domain_types_dir, hooks_tmp.path(), &output_dir)
            .await;

    // Response: the derived field stays visible but is marked computed/readonly.
    assert!(
        response.contains("pub candidate_id"),
        "response must still expose candidate_id:\n{response}"
    );
    assert!(
        response.contains("Computed by the mox domain model"),
        "response must mark candidate_id as computed/read-only:\n{response}"
    );

    // Create/update: the derived field is omitted entirely.
    assert!(
        !create.contains("pub candidate_id"),
        "create DTO must not expose derived candidate_id:\n{create}"
    );
    assert!(
        !update.contains("pub candidate_id"),
        "update DTO must not expose derived candidate_id:\n{update}"
    );

    // Control: a non-derived sibling field is untouched in all three DTOs.
    for (name, content) in [
        ("response", &response),
        ("create", &create),
        ("update", &update),
    ] {
        assert!(
            content.contains("pub status"),
            "{name} DTO must keep the non-derived status field"
        );
    }
}

/// Presence-gating (the byte-identity contract): with NO mox ingested, the
/// same pipeline produces DTOs identical to the pre-feature output —
/// candidate_id present in create/update, no computed markers anywhere.
#[tokio::test]
async fn without_mox_ingest_dtos_are_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let output_dir = dir.path().join("out");
    let domain_types_dir = dir.path().join("domain-types");
    fs::create_dir_all(&output_dir).unwrap();

    let engine = GrafeoEngine::in_memory().unwrap();
    let config =
        codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml"))
            .unwrap();
    ingest_fixture_schemas(&engine, &config).await;

    let hooks_tmp = tempfile::tempdir().unwrap();
    let (response, create, update) =
        run_generation_and_read_dtos(&engine, &domain_types_dir, hooks_tmp.path(), &output_dir)
            .await;

    for (name, content) in [
        ("dto_response.rs", &response),
        ("dto_create.rs", &create),
        ("dto_update.rs", &update),
    ] {
        assert!(
            content.contains("pub candidate_id"),
            "{name} must keep candidate_id when no mox is ingested"
        );
        assert!(
            !content.contains("Computed by the mox domain model"),
            "{name} must not carry computed markers when no mox is ingested"
        );
    }
}
