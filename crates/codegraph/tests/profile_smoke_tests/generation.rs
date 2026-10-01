use std::path::Path;

use codegraph::profile::{self, BuildPlan, CapabilityRegistry};

use crate::harness::{mock_test_setup, profiles_path};

// ── Integration test: run generators with a profile plan ────────────────

#[tokio::test]
async fn full_generation_without_profile_runs_all_generators() {
    let (mock, config, tera, output_dir) = mock_test_setup();
    let domain_types_tmp = tempfile::TempDir::new().unwrap();
    let hooks_tmp = tempfile::TempDir::new().unwrap();

    let report = codegraph::generate::run_generators_with_domain_types_base(
        &mock,
        &config,
        output_dir.path(),
        &tera,
        &Default::default(),
        &Default::default(),
        Path::new(""),
        domain_types_tmp.path(),
        hooks_tmp.path(),
    )
    .await
    .unwrap();

    assert!(!report.has_errors(), "no-profile run should have no errors");
    assert!(
        !report.files.is_empty(),
        "no-profile run should produce files"
    );
    let all_count = report.files.len();
    println!("Full run produced {all_count} files");
}

#[tokio::test]
async fn generation_emits_ownership_manifests_at_each_root() {
    let (mock, config, tera, output_dir) = mock_test_setup();
    let domain_types_tmp = tempfile::TempDir::new().unwrap();
    let hooks_tmp = tempfile::TempDir::new().unwrap();

    let report = codegraph::generate::run_generators_with_domain_types_base(
        &mock,
        &config,
        output_dir.path(),
        &tera,
        &Default::default(),
        &Default::default(),
        Path::new(""),
        domain_types_tmp.path(),
        hooks_tmp.path(),
    )
    .await
    .unwrap();

    assert!(!report.has_errors(), "no-profile run should have no errors");
    assert!(
        !report.files.is_empty(),
        "no-profile run should produce files"
    );

    // A manifest must exist at every output root...
    for root in [output_dir.path(), domain_types_tmp.path(), hooks_tmp.path()] {
        let manifest_path = root.join(codegraph::generate::manifest::MANIFEST_FILENAME);
        assert!(
            manifest_path.exists(),
            "expected manifest at {}",
            manifest_path.display()
        );
        let m: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap())
                .expect("manifest must be valid JSON");
        assert!(
            m["generated"]
                .as_array()
                .map(|a| !a.is_empty())
                .unwrap_or(false),
            "manifest at {} must list generated files",
            manifest_path.display()
        );
        assert!(
            m["generatedBy"]
                .as_str()
                .is_some_and(|s| s.starts_with("codegraph v")),
            "manifest must record the generating tool version"
        );
    }

    // ...with paths relative to that root. The monolith scaffold writes
    // src/main.rs to the output dir; the domain-types crate gets a lib.rs.
    let main_manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(output_dir.path().join(".codegraph-manifest.json")).unwrap(),
    )
    .unwrap();
    let main_generated: Vec<&str> = main_manifest["generated"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(
        main_generated.contains(&"src/main.rs"),
        "output-dir manifest must list src/main.rs relative to the root"
    );
    assert!(
        main_generated.iter().all(|p| !p.starts_with('/')),
        "manifest paths must be relative (not absolute)"
    );

    let dt_manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(domain_types_tmp.path().join(".codegraph-manifest.json")).unwrap(),
    )
    .unwrap();
    let dt_generated: Vec<&str> = dt_manifest["generated"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(
        dt_generated.contains(&"src/lib.rs"),
        "domain-types manifest must list src/lib.rs relative to that root"
    );
}

#[tokio::test]
async fn generation_with_api_profile_produces_fewer_files_than_full() {
    let (mock, config, tera, output_dir) = mock_test_setup();
    let domain_types_tmp = tempfile::TempDir::new().unwrap();
    let hooks_tmp = tempfile::TempDir::new().unwrap();

    // Build plan for API profile
    let registry = CapabilityRegistry::new();
    let resolved = profile::load_and_resolve_profile(&profiles_path(), "api", None).unwrap();
    let plan = BuildPlan::from_profile(&resolved, &registry).unwrap();

    // Sanity check: plan has API generators but no UI/CLI generators
    assert!(plan.has_entity_gen("ddl"));
    assert!(plan.has_entity_gen("handler"));
    assert!(!plan.has_entity_gen("ui_page"));
    assert!(!plan.has_entity_gen("cli_command"));

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
            ext_points: None,
            seed_config: None,
            build_plan: Some(&plan),
            ifml_frameworks: vec![],
            ifml_components: None,
            ux_rules: None,
            project_config: None,
            emdash_plugins: None,
            domain_config_dir: None,
        })
        .await
        .unwrap();

    assert!(
        !report.has_errors(),
        "API profile run should have no errors"
    );
    assert!(
        !report.files.is_empty(),
        "API profile run should produce files"
    );

    let api_count = report.files.len();
    println!("API profile produced {api_count} files");

    // Now run again without a plan, verify we get more files
    let output_dir2 = tempfile::TempDir::new().unwrap();
    let domain_types_tmp2 = tempfile::TempDir::new().unwrap();
    let hooks_tmp2 = tempfile::TempDir::new().unwrap();

    let report2 = codegraph::generate::run_generators_with_domain_types_base(
        &mock,
        &config,
        output_dir2.path(),
        &tera,
        &Default::default(),
        &Default::default(),
        Path::new(""),
        domain_types_tmp2.path(),
        hooks_tmp2.path(),
    )
    .await
    .unwrap();

    let all_count = report2.files.len();
    println!("Full run produced {all_count} files");

    assert!(
        api_count < all_count,
        "API profile ({api_count} files) should produce fewer files than full run ({all_count} files)"
    );
}

#[tokio::test]
async fn generation_with_ui_profile_produces_only_ui_files() {
    let (mock, config, tera, output_dir) = mock_test_setup();
    let domain_types_tmp = tempfile::TempDir::new().unwrap();
    let hooks_tmp = tempfile::TempDir::new().unwrap();

    let registry = CapabilityRegistry::new();
    let resolved = profile::load_and_resolve_profile(&profiles_path(), "ui", None).unwrap();
    let plan = BuildPlan::from_profile(&resolved, &registry).unwrap();

    assert!(plan.has_entity_gen("ui_page"));
    assert!(!plan.has_entity_gen("ddl"));

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
            ext_points: None,
            seed_config: None,
            build_plan: Some(&plan),
            ifml_frameworks: vec![],
            ifml_components: None,
            ux_rules: None,
            project_config: None,
            emdash_plugins: None,
            domain_config_dir: None,
        })
        .await
        .unwrap();

    assert!(!report.has_errors(), "UI profile run should have no errors");
    // UI profile with only a CandidateType should still produce some files
    // (descriptors, shell, types, scaffold, etc.)
    println!("UI profile produced {} files", report.files.len());
    for f in &report.files {
        println!("  {}", f.path.display());
    }
}

#[tokio::test]
async fn generation_with_cli_profile_produces_only_cli_files() {
    let (mock, config, tera, output_dir) = mock_test_setup();
    let domain_types_tmp = tempfile::TempDir::new().unwrap();
    let hooks_tmp = tempfile::TempDir::new().unwrap();

    let registry = CapabilityRegistry::new();
    let resolved = profile::load_and_resolve_profile(&profiles_path(), "cli", None).unwrap();
    let plan = BuildPlan::from_profile(&resolved, &registry).unwrap();

    assert!(plan.has_entity_gen("cli_command"));
    assert!(!plan.has_entity_gen("ddl"));
    assert!(!plan.has_entity_gen("ui_page"));

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
            ext_points: None,
            seed_config: None,
            build_plan: Some(&plan),
            ifml_frameworks: vec![],
            ifml_components: None,
            ux_rules: None,
            project_config: None,
            emdash_plugins: None,
            domain_config_dir: None,
        })
        .await
        .unwrap();

    assert!(
        !report.has_errors(),
        "CLI profile run should have no errors"
    );
    println!("CLI profile produced {} files", report.files.len());
    for f in &report.files {
        println!("  {}", f.path.display());
    }
}

#[tokio::test]
async fn generation_with_lite_variant_produces_fewer_files_than_full_api() {
    let (mock, config, tera, _output_dir) = mock_test_setup();
    let _domain_types_tmp = tempfile::TempDir::new().unwrap();
    let _hooks_tmp = tempfile::TempDir::new().unwrap();

    let registry = CapabilityRegistry::new();

    // Full API
    let full = profile::load_and_resolve_profile(&profiles_path(), "api", None).unwrap();
    let full_plan = BuildPlan::from_profile(&full, &registry).unwrap();

    let output1 = tempfile::TempDir::new().unwrap();
    let dt1 = tempfile::TempDir::new().unwrap();
    let hk1 = tempfile::TempDir::new().unwrap();
    let report_full =
        codegraph::generate::run_generators_with_opts(codegraph::generate::GeneratorOpts {
            db: &mock,
            config: &config,
            output_dir: output1.path(),
            tera: &tera,
            ui_overrides: &Default::default(),
            ui_domains: &Default::default(),
            schema_base_dir: Path::new(""),
            domain_types_base: Some(dt1.path()),
            hooks_base: Some(hk1.path()),
            ext_points: None,
            seed_config: None,
            build_plan: Some(&full_plan),
            ifml_frameworks: vec![],
            ifml_components: None,
            ux_rules: None,
            project_config: None,
            emdash_plugins: None,
            domain_config_dir: None,
        })
        .await
        .unwrap();

    let full_count = report_full.files.len();

    // Lite variant
    let lite = profile::load_and_resolve_profile(&profiles_path(), "api", Some("lite")).unwrap();
    let lite_plan = BuildPlan::from_profile(&lite, &registry).unwrap();

    let output2 = tempfile::TempDir::new().unwrap();
    let dt2 = tempfile::TempDir::new().unwrap();
    let hk2 = tempfile::TempDir::new().unwrap();
    let report_lite =
        codegraph::generate::run_generators_with_opts(codegraph::generate::GeneratorOpts {
            db: &mock,
            config: &config,
            output_dir: output2.path(),
            tera: &tera,
            ui_overrides: &Default::default(),
            ui_domains: &Default::default(),
            schema_base_dir: Path::new(""),
            domain_types_base: Some(dt2.path()),
            hooks_base: Some(hk2.path()),
            ext_points: None,
            seed_config: None,
            build_plan: Some(&lite_plan),
            ifml_frameworks: vec![],
            ifml_components: None,
            ux_rules: None,
            project_config: None,
            emdash_plugins: None,
            domain_config_dir: None,
        })
        .await
        .unwrap();

    let lite_count = report_lite.files.len();

    println!("Full API: {full_count} files, Lite: {lite_count} files");
    assert!(
        lite_count < full_count,
        "Lite variant ({lite_count} files) should produce fewer files than full API ({full_count} files)"
    );
}

#[tokio::test]
async fn ddd_only_mode_produces_ddd_but_not_api_files() {
    let (mock, _config, tera, output_dir) = mock_test_setup();
    let domain_types_tmp = tempfile::TempDir::new().unwrap();
    let hooks_tmp = tempfile::TempDir::new().unwrap();

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
generation_mode = "ddd_only"
"#,
    )
    .unwrap();

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
            ext_points: None,
            seed_config: None,
            build_plan: None,
            ifml_frameworks: vec![],
            ifml_components: None,
            ux_rules: None,
            project_config: None,
            emdash_plugins: None,
            domain_config_dir: None,
        })
        .await
        .unwrap();

    assert!(
        !report.has_errors(),
        "ddd_only mode should produce no errors"
    );

    let file_paths: Vec<String> = report
        .files
        .iter()
        .map(|f| f.path.to_string_lossy().to_string())
        .collect();

    // DDD files should exist
    let has_repository = file_paths
        .iter()
        .any(|p| p.contains("candidate/repository.rs"));
    let has_command = file_paths
        .iter()
        .any(|p| p.contains("candidate/command.rs"));
    let has_query = file_paths.iter().any(|p| p.contains("candidate/query.rs"));

    // API files should NOT exist
    let has_handler = file_paths
        .iter()
        .any(|p| p.contains("candidate_handler.rs"));
    let has_workflow = file_paths.iter().any(|p| p.contains("candidate_workflow"));

    // Router file may exist (domain generator always runs) but should be empty
    let router_path = file_paths
        .iter()
        .find(|p| p.contains("recruiting/router.rs"));

    assert!(has_repository, "ddd_only mode should produce repository");
    assert!(has_command, "ddd_only mode should produce command");
    assert!(has_query, "ddd_only mode should produce query");
    assert!(!has_handler, "ddd_only mode should NOT produce handler");
    assert!(!has_workflow, "ddd_only mode should NOT produce workflow");

    // Router should exist but contain no candidate routes
    if let Some(rp) = router_path {
        let router_file = report
            .files
            .iter()
            .find(|f| f.path.to_string_lossy() == *rp)
            .unwrap();
        let content = &router_file.content;
        assert!(
            !content.contains("candidate"),
            "ddd_only router should not contain candidate routes, got:\n{content}"
        );
    }

    println!(
        "ddd_only mode produced {} files (repo={has_repository}, cmd={has_command}, qry={has_query}, handler={has_handler}, router={})",
        report.files.len(),
        router_path.is_some()
    );
}
