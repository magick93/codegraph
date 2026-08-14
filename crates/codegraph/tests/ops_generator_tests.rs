//! Tests for the `ops` global generator.
//!
//! Verifies the generated `codegraph-ops.toml` manifest and `testkit/` crate
//! are produced by the full generation pipeline and that the manifest
//! serializes back into the `OpsManifest` type used by the codegraph-ops
//! harness.

#[path = "test_framework/mod.rs"]
mod test_framework;

use std::path::Path;

use codegraph::generate::traits::GlobalGenerator;
use codegraph_core::types::{PropertyNode, SchemaNode};
use test_framework::validators::file_presence::FilePresenceValidator;
use test_framework::validators::string_pattern::StringPatternValidator;
use test_framework::GeneratorTest;

/// Minimal mock engine + config, same pattern as profile_smoke_tests.
/// No `depends_on` so the domain registry stays acyclic with one domain.
fn mock_test_setup() -> (
    codegraph_core::mock::MockEngine,
    codegraph_config::DomainConfig,
    tera::Tera,
    tempfile::TempDir,
) {
    let schema = SchemaNode {
        schema_id: "recruiting/json/CandidateType.json".to_string(),
        title: "CandidateType".to_string(),
        description: Some("A candidate for a position".to_string()),
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("recruiting".to_string()),
        rel_path: "recruiting/json/CandidateType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "Candidate".to_string(),
        pg_table_name: "candidate".to_string(),
        api_path_segment: "candidates".to_string(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: true,
        has_one_of: false,
        has_any_of: false,
        has_definitions: true,
    };

    let props = vec![PropertyNode {
        name: "givenName".to_string(),
        prop_type: "string".to_string(),
        description: Some("First name".to_string()),
        format: None,
        is_required: true,
        is_nullable: false,
        is_array: false,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: "given_name".to_string(),
        pg_column_type: "TEXT".to_string(),
        rust_field_name: "given_name".to_string(),
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
    }];

    let engine = codegraph_core::mock::MockEngine::builder()
        .with_schema(schema)
        .with_properties("CandidateType", props)
        .build();

    let config = codegraph_config::config::parse_domain_config_str(
        r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting"
postgres_schema = "recruiting"
entities = ["CandidateType"]

[domains.recruiting.entity_config.CandidateType]
operations = ["create", "read", "update", "delete", "list"]
"#,
    )
    .unwrap();

    let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = codegraph::generate::template_engine::create_tera(&template_dir).unwrap();

    let output_dir = tempfile::TempDir::new().unwrap();

    (engine, config, tera, output_dir)
}

/// The full pipeline (no build plan → all generators) must produce the ops
/// manifest plus the testkit crate, with manifest content present.
#[test]
fn ops_generator_produces_manifest_and_testkit_in_full_pipeline() {
    let (engine, config, tera, output_dir) = mock_test_setup();

    let test = GeneratorTest {
        db: &engine,
        config: &config,
        tera: &tera,
        output_dir: output_dir.path(),
        validators: vec![
            Box::new(FilePresenceValidator {
                label: "ops_check".to_string(),
                required_paths: vec![
                    "codegraph-ops.toml".to_string(),
                    "testkit/Cargo.toml".to_string(),
                    "testkit/src/main.rs".to_string(),
                ],
            }),
            Box::new(StringPatternValidator {
                label: "ops_manifest_content".to_string(),
                required_patterns: vec![
                    "app_name".to_string(),
                    "capabilities".to_string(),
                    "output_dir = \".\"".to_string(),
                ],
                forbidden_patterns: vec![],
            }),
        ],
    };

    let files = test.run().expect("generation failed");
    assert!(!files.is_empty(), "pipeline should produce files");

    let manifest = files
        .iter()
        .find(|f| f.path == Path::new("codegraph-ops.toml"))
        .expect("codegraph-ops.toml should be collected");
    assert!(
        manifest.content.contains("DO NOT EDIT"),
        "manifest should carry the DO NOT EDIT header"
    );
}

/// Direct generator call: the manifest must round-trip through the
/// `OpsManifest` serde types used by the codegraph-ops harness.
#[tokio::test]
async fn ops_generator_direct_call_manifest_roundtrips() {
    let (engine, config, tera, output_dir) = mock_test_setup();

    let gen = codegraph::generate::ops::OpsManifestGenerator::new(
        output_dir.path(),
        true, // has_cli
        true, // has_ui
        true, // has_admin_cli
        true, // has_grpc
    );
    let files = gen
        .generate(
            &engine,
            &config,
            &[],
            &tera,
            &codegraph::generate::ProjectConfig::default(),
        )
        .await
        .expect("ops generator failed");

    assert_eq!(files.len(), 3, "manifest + testkit Cargo.toml + main.rs");

    let manifest = files
        .iter()
        .find(|f| f.path.ends_with("codegraph-ops.toml"))
        .expect("codegraph-ops.toml");
    let parsed: codegraph_config::ops_manifest::OpsManifest =
        toml::from_str(&manifest.content).expect("manifest should parse as OpsManifest");

    assert_eq!(parsed.app_name, "app");
    assert_eq!(parsed.output_dir, Path::new("."));
    assert_eq!(parsed.servers.api_port, 3000);
    assert_eq!(parsed.servers.ui_port, 5173);
    assert_eq!(parsed.servers.bind_addr, "0.0.0.0");
    assert_eq!(parsed.database.api.host, "localhost");
    assert_eq!(parsed.database.api.port, 5432);
    let e2e = parsed.database.e2e.as_ref().expect("e2e db target");
    assert_eq!(e2e.port, 54322);
    assert!(parsed.database.e2e_app.is_none());
    assert!(parsed.supabase.is_none());
    assert!(parsed.hurl.is_none());
    assert!(parsed.hooks.is_empty());
    assert!(parsed.extensions.is_empty());
    assert!(parsed.capabilities.has_cli);
    assert!(parsed.capabilities.has_ui);
    assert!(parsed.capabilities.has_admin_cli);
    assert!(parsed.capabilities.has_grpc);
    assert_eq!(parsed.capabilities.database_target, "postgres");
    assert_eq!(parsed.capabilities.persistence_provider, "sea_orm");

    let cargo = files
        .iter()
        .find(|f| f.path.ends_with("testkit/Cargo.toml"))
        .expect("testkit/Cargo.toml");
    assert!(
        cargo.content.contains("codegraph-ops"),
        "testkit Cargo.toml should depend on codegraph-ops. Got:\n{}",
        cargo.content
    );
    assert!(
        cargo.content.contains("tokio"),
        "testkit Cargo.toml should depend on tokio"
    );

    let main = files
        .iter()
        .find(|f| f.path.ends_with("testkit/src/main.rs"))
        .expect("testkit/src/main.rs");
    assert!(
        main.content.contains("codegraph_ops"),
        "testkit main.rs should reference codegraph_ops. Got:\n{}",
        main.content
    );
}

/// The testkit Cargo.toml must pin the codegraph-ops dependency to the
/// codegraph git rev when `project.codegraph_rev` is set (external consumers
/// depend on codegraph crates via git, so the relative path fallback would
/// not exist in their repo).
#[tokio::test]
async fn testkit_cargo_uses_git_rev_when_pinned() {
    let (engine, config, tera, output_dir) = mock_test_setup();

    let gen = codegraph::generate::ops::OpsManifestGenerator::new(
        output_dir.path(),
        true, // has_cli
        true, // has_ui
        true, // has_admin_cli
        true, // has_grpc
    );
    let project = codegraph::generate::ProjectConfig {
        codegraph_rev: "abc123".into(),
        ..codegraph::generate::ProjectConfig::default()
    };
    let files = gen
        .generate(&engine, &config, &[], &tera, &project)
        .await
        .expect("ops generator failed");

    let cargo = files
        .iter()
        .find(|f| f.path.ends_with("testkit/Cargo.toml"))
        .expect("testkit/Cargo.toml");
    assert!(
        cargo
            .content
            .contains(r#"git = "https://github.com/magick93/codegraph.git""#),
        "testkit Cargo.toml should pin codegraph-ops via git. Got:\n{}",
        cargo.content
    );
    assert!(
        cargo.content.contains(r#"rev = "abc123""#),
        "testkit Cargo.toml should pin the codegraph rev. Got:\n{}",
        cargo.content
    );
    assert!(
        !cargo.content.contains("path ="),
        "testkit Cargo.toml should not use a path dependency when a rev is pinned. Got:\n{}",
        cargo.content
    );
}

/// With an empty `project.codegraph_rev` (the default), the testkit Cargo.toml
/// falls back to the path dependency into the codegraph workspace.
#[tokio::test]
async fn testkit_cargo_uses_path_when_rev_empty() {
    let (engine, config, tera, output_dir) = mock_test_setup();

    let gen = codegraph::generate::ops::OpsManifestGenerator::new(
        output_dir.path(),
        true, // has_cli
        true, // has_ui
        true, // has_admin_cli
        true, // has_grpc
    );
    let files = gen
        .generate(
            &engine,
            &config,
            &[],
            &tera,
            &codegraph::generate::ProjectConfig::default(),
        )
        .await
        .expect("ops generator failed");

    let cargo = files
        .iter()
        .find(|f| f.path.ends_with("testkit/Cargo.toml"))
        .expect("testkit/Cargo.toml");
    assert!(
        cargo.content.contains("path ="),
        "testkit Cargo.toml should use a path dependency when no rev is pinned. Got:\n{}",
        cargo.content
    );
    assert!(
        !cargo.content.contains("rev ="),
        "testkit Cargo.toml should not pin a git rev when codegraph_rev is empty. Got:\n{}",
        cargo.content
    );
}

/// The emitted testkit crate must actually compile. The codegraph-ops path
/// dependency is rewritten to an absolute path so the test is hermetic
/// regardless of where the tempdir lives (the generated relative path only
/// resolves when the output sits inside the codegraph workspace).
#[tokio::test]
#[ignore = "slow: compiles codegraph-ops; run in CI"]
async fn testkit_crate_compiles() {
    let (engine, config, tera, output_dir) = mock_test_setup();

    let gen = codegraph::generate::ops::OpsManifestGenerator::new(
        output_dir.path(),
        true, // has_cli
        true, // has_ui
        true, // has_admin_cli
        true, // has_grpc
    );
    let files = gen
        .generate(
            &engine,
            &config,
            &[],
            &tera,
            &codegraph::generate::ProjectConfig::default(),
        )
        .await
        .expect("ops generator failed");

    let manifest = files
        .iter()
        .find(|f| f.path.ends_with("codegraph-ops.toml"))
        .expect("codegraph-ops.toml");
    let cargo = files
        .iter()
        .find(|f| f.path.ends_with("testkit/Cargo.toml"))
        .expect("testkit/Cargo.toml");
    let main = files
        .iter()
        .find(|f| f.path.ends_with("testkit/src/main.rs"))
        .expect("testkit/src/main.rs");

    let ops_abs = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.join("codegraph-ops"))
        .expect("codegraph workspace crates dir");
    let ops_abs = ops_abs
        .canonicalize()
        .expect("codegraph-ops crate should exist in the workspace");

    let cargo_content = cargo
        .content
        .lines()
        .map(|line| {
            if line.starts_with("codegraph-ops") {
                format!("codegraph-ops = {{ path = \"{}\" }}", ops_abs.display())
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let root = output_dir.path();
    std::fs::write(root.join("codegraph-ops.toml"), &manifest.content).expect("write manifest");
    std::fs::create_dir_all(root.join("testkit").join("src")).expect("mkdir testkit/src");
    std::fs::write(root.join("testkit").join("Cargo.toml"), &cargo_content)
        .expect("write Cargo.toml");
    std::fs::write(
        root.join("testkit").join("src").join("main.rs"),
        &main.content,
    )
    .expect("write main.rs");

    let status = std::process::Command::new("cargo")
        .arg("build")
        .arg("--manifest-path")
        .arg(root.join("testkit").join("Cargo.toml"))
        .arg("--target-dir")
        .arg(root.join("target"))
        .status()
        .expect("spawn cargo build");
    assert!(status.success(), "cargo build of generated testkit failed");
}

/// Capability flags must mirror the build plan: only the grpc scaffold flag
/// is off when grpc is not in the plan.
#[tokio::test]
async fn ops_generator_capability_flags_reflect_constructor() {
    let (engine, config, tera, output_dir) = mock_test_setup();

    let gen = codegraph::generate::ops::OpsManifestGenerator::new(
        output_dir.path(),
        false, // has_cli
        false, // has_ui
        false, // has_admin_cli
        false, // has_grpc
    );
    let files = gen
        .generate(
            &engine,
            &config,
            &[],
            &tera,
            &codegraph::generate::ProjectConfig::default(),
        )
        .await
        .expect("ops generator failed");

    let manifest = files
        .iter()
        .find(|f| f.path.ends_with("codegraph-ops.toml"))
        .expect("codegraph-ops.toml");
    let parsed: codegraph_config::ops_manifest::OpsManifest =
        toml::from_str(&manifest.content).expect("manifest should parse");

    assert!(!parsed.capabilities.has_cli);
    assert!(!parsed.capabilities.has_ui);
    assert!(!parsed.capabilities.has_admin_cli);
    assert!(!parsed.capabilities.has_grpc);
}

/// End-to-end contract: the emitted manifest must load through the harness's
/// own `OpsConfig` (paths resolved, db targets wrapped, api URL derived).
#[tokio::test]
async fn emitted_manifest_loads_through_ops_config() {
    let (engine, config, tera, output_dir) = mock_test_setup();

    let gen = codegraph::generate::ops::OpsManifestGenerator::new(
        output_dir.path(),
        true,
        true,
        true,
        true,
    );
    let files = gen
        .generate(
            &engine,
            &config,
            &[],
            &tera,
            &codegraph::generate::ProjectConfig::default(),
        )
        .await
        .expect("ops generator failed");

    // Write the emitted manifest to disk so OpsConfig::load can read it.
    let manifest_path = output_dir.path().join("codegraph-ops.toml");
    let manifest = files
        .iter()
        .find(|f| f.path.ends_with("codegraph-ops.toml"))
        .expect("codegraph-ops.toml");
    std::fs::write(&manifest_path, &manifest.content).expect("write manifest");

    let cfg = codegraph_ops::OpsConfig::load(&manifest_path).expect("OpsConfig::load");

    assert_eq!(cfg.app_binary_name(), "app");
    assert_eq!(cfg.api_url(), "http://localhost:3000");
    assert_eq!(cfg.api_db.port, 5432);
    assert!(cfg.e2e_db.is_some());
    assert!(cfg.e2e_app_db.is_none());
    assert!(cfg.supabase_dir.is_none());
    assert!(cfg.hurl_dir.is_none());
    assert!(cfg.hooks.is_empty());
}
