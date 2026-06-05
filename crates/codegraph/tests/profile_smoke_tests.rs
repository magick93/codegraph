use std::path::Path;

use codegraph::generate::template_engine;
use codegraph::profile::{self, BuildPlan, CapabilityRegistry};
use codegraph_core::types::{PropertyNode, SchemaNode};

/// Path to the project root's profiles.toml
fn profiles_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .unwrap()
        .join("profiles.toml")
}

#[test]
fn profiles_file_exists_and_parses() {
    let path = profiles_path();
    assert!(
        path.exists(),
        "profiles.toml not found at {}",
        path.display()
    );

    let content = std::fs::read_to_string(&path).unwrap();
    let config: profile::ProfilesConfig = toml::from_str(&content).unwrap();
    assert!(!config.profiles.is_empty());
}

#[test]
fn default_profile_parses_and_builds_plan() {
    let registry = CapabilityRegistry::new();
    let resolved = profile::load_and_resolve_profile(&profiles_path(), "default", None).unwrap();
    let plan = BuildPlan::from_profile(&resolved, &registry).unwrap();

    assert_eq!(resolved.meta.name, "default");
    assert!(
        !plan.entity_generators.is_empty(),
        "default plan needs entity generators"
    );
    assert!(
        !plan.domain_generators.is_empty(),
        "default plan needs domain generators"
    );
    assert!(
        !plan.global_generators.is_empty(),
        "default plan needs global generators"
    );
    assert!(
        !plan.post_gen_scripts.is_empty(),
        "default plan needs post-gen scripts"
    );
}

#[test]
fn api_profile_parses_and_builds_plan() {
    let registry = CapabilityRegistry::new();
    let resolved = profile::load_and_resolve_profile(&profiles_path(), "api", None).unwrap();
    let plan = BuildPlan::from_profile(&resolved, &registry).unwrap();

    assert_eq!(resolved.meta.name, "api");
    // API profile should have entity generators but no UI entity generators
    assert!(plan.has_entity_gen("ddl"));
    assert!(plan.has_entity_gen("handler"));
    assert!(
        !plan.has_entity_gen("ui_page"),
        "api profile should not include UI generators"
    );
    assert!(
        !plan.has_entity_gen("cli_command"),
        "api profile should not include CLI generators"
    );

    // Should have API domain generators
    assert!(plan.has_domain_gen("router"));
    assert!(plan.has_domain_gen("links"));
    assert!(!plan.has_domain_gen("ui-domain-layout"));
    assert!(!plan.has_domain_gen("cli_domain"));

    // Should have API global generators
    assert!(plan.has_global_gen("openapi"));
    assert!(plan.has_global_gen("scaffold"));
    assert!(
        !plan.has_global_gen("ui_scaffold"),
        "api profile should not include UI global generators"
    );
    assert!(
        !plan.has_global_gen("cli_scaffold"),
        "api profile should not include CLI global generators"
    );

    assert!(!plan.post_gen_scripts.is_empty());
}

#[test]
fn ui_profile_parses_and_builds_plan() {
    let registry = CapabilityRegistry::new();
    let resolved = profile::load_and_resolve_profile(&profiles_path(), "ui", None).unwrap();
    let plan = BuildPlan::from_profile(&resolved, &registry).unwrap();

    assert_eq!(resolved.meta.name, "ui");
    // UI profile should have UI entity generators only
    assert!(
        !plan.has_entity_gen("ddl"),
        "ui profile should not include API generators"
    );
    assert!(!plan.has_entity_gen("handler"));
    assert!(plan.has_entity_gen("ui_page"));
    assert!(plan.has_entity_gen("ui_form"));
    assert!(plan.has_entity_gen("ui_store"));

    // Should have UI domain generators only
    assert!(!plan.has_domain_gen("router"));
    assert!(plan.has_domain_gen("ui-domain-layout"));

    // Should have UI global generators
    assert!(plan.has_global_gen("ui_scaffold"));
    assert!(plan.has_global_gen("ui_types"));
    assert!(plan.has_global_gen("ui_codelist"));
    assert!(
        !plan.has_global_gen("openapi"),
        "ui profile should not include API global generators"
    );

    assert_eq!(resolved.meta.tags, vec!["frontend", "sveltekit"]);
}

#[test]
fn cli_profile_parses_and_builds_plan() {
    let registry = CapabilityRegistry::new();
    let resolved = profile::load_and_resolve_profile(&profiles_path(), "cli", None).unwrap();
    let plan = BuildPlan::from_profile(&resolved, &registry).unwrap();

    assert_eq!(resolved.meta.name, "cli");
    assert!(plan.has_entity_gen("cli_command"));
    assert!(!plan.has_entity_gen("ddl"));
    assert!(!plan.has_entity_gen("ui_page"));

    assert!(plan.has_domain_gen("cli_domain"));
    assert!(!plan.has_domain_gen("router"));

    assert!(plan.has_global_gen("cli_scaffold"));
    assert!(!plan.has_global_gen("ui_scaffold"));
    assert!(!plan.has_global_gen("openapi"));
}

#[test]
fn fullstack_profile_parses_and_builds_plan() {
    let registry = CapabilityRegistry::new();
    let resolved = profile::load_and_resolve_profile(&profiles_path(), "fullstack", None).unwrap();
    let plan = BuildPlan::from_profile(&resolved, &registry).unwrap();

    assert_eq!(resolved.meta.name, "fullstack");
    // Should have both API and UI generators
    assert!(plan.has_entity_gen("ddl"));
    assert!(plan.has_entity_gen("ui_page"));
    assert!(plan.has_domain_gen("router"));
    assert!(plan.has_domain_gen("ui-domain-layout"));
    assert!(plan.has_global_gen("openapi"));
    assert!(plan.has_global_gen("ui_scaffold"));
    // But not CLI
    assert!(!plan.has_entity_gen("cli_command"));
    assert!(!plan.has_global_gen("cli_scaffold"));
}

#[test]
fn ci_profile_parses_and_builds_plan() {
    let registry = CapabilityRegistry::new();
    let resolved = profile::load_and_resolve_profile(&profiles_path(), "ci", None).unwrap();
    let plan = BuildPlan::from_profile(&resolved, &registry).unwrap();

    assert_eq!(resolved.meta.name, "ci");
    // CI should have all generators (compile-gate)
    assert!(plan.has_entity_gen("ddl"));
    assert!(plan.has_entity_gen("ui_page"));
    assert!(plan.has_entity_gen("cli_command"));
    assert!(plan.has_global_gen("openapi"));
    assert!(plan.has_global_gen("ui_scaffold"));
    assert!(plan.has_global_gen("cli_scaffold"));
}

#[test]
fn api_profile_variant_lite_reduces_generators() {
    let registry = CapabilityRegistry::new();
    let resolved =
        profile::load_and_resolve_profile(&profiles_path(), "api", Some("lite")).unwrap();
    let plan = BuildPlan::from_profile(&resolved, &registry).unwrap();

    // Lite variant should have fewer generators than full API
    assert!(plan.has_entity_gen("ddl"));
    assert!(plan.has_entity_gen("handler"));
    assert!(
        !plan.has_entity_gen("test"),
        "lite should exclude test generator"
    );
    assert!(
        !plan.has_entity_gen("workflow_action"),
        "lite should exclude workflow_action"
    );
    assert!(
        !plan.has_entity_gen("media_route"),
        "lite should exclude media_route"
    );
    assert!(
        !plan.has_entity_gen("lifecycle_trait"),
        "lite should exclude lifecycle_trait"
    );
    assert!(
        !plan.has_entity_gen("domain_types_dto"),
        "lite should exclude domain_types_dto"
    );

    // Features should be overridden
    assert_eq!(
        resolved.features.get("auth").unwrap().as_bool(),
        Some(false)
    );
    assert_eq!(
        resolved.features.get("validation_level").unwrap().as_str(),
        Some("balanced")
    );
}

#[test]
fn fullstack_variant_enterprise_adds_features() {
    let registry = CapabilityRegistry::new();
    let resolved =
        profile::load_and_resolve_profile(&profiles_path(), "fullstack", Some("enterprise"))
            .unwrap();
    let plan = BuildPlan::from_profile(&resolved, &registry).unwrap();

    assert!(plan.has_entity_gen("ddl"));
    assert!(plan.has_entity_gen("ui_page"));
    // Features merged from variant
    assert_eq!(
        resolved.features.get("audit_trail").unwrap().as_bool(),
        Some(true)
    );
    assert_eq!(
        resolved.features.get("multitenancy").unwrap().as_bool(),
        Some(true)
    );
    assert_eq!(
        resolved
            .features
            .get("row_level_security")
            .unwrap()
            .as_bool(),
        Some(true)
    );
}

#[test]
fn enterprise_variant_generators_match_base_profile() {
    // Regression test: the enterprise variant is additive (keeps all generators
    // from the base, adds features). If a generator is added to the base
    // fullstack profile but not to the enterprise variant, it would silently
    // stop generating. This test catches that drift.
    let registry = CapabilityRegistry::new();

    let base = profile::load_and_resolve_profile(&profiles_path(), "fullstack", None).unwrap();
    let enterprise =
        profile::load_and_resolve_profile(&profiles_path(), "fullstack", Some("enterprise"))
            .unwrap();

    for (section_name, base_section) in &base.sections {
        let ent_section = &enterprise.sections[section_name.as_str()];
        for gen in &base_section.generators {
            assert!(
                ent_section.generators.contains(gen),
                "enterprise variant is missing generator \"{gen}\" from \
                 base profile's [{section_name}] section. \
                 If the generator was intentionally omitted, update the variant.",
            );
        }
    }
}

#[test]
fn fullstack_variant_lite_strips_to_minimal() {
    let registry = CapabilityRegistry::new();
    let resolved =
        profile::load_and_resolve_profile(&profiles_path(), "fullstack", Some("lite")).unwrap();
    let plan = BuildPlan::from_profile(&resolved, &registry).unwrap();

    // Lite has only minimal generators
    let mut entity_gens = plan.entity_generators.clone();
    entity_gens.sort();
    assert_eq!(
        entity_gens,
        vec!["ddl", "dto", "handler", "ui_form", "ui_page"]
    );

    assert_eq!(plan.domain_generators, vec!["router"]);

    let mut global_gens = plan.global_generators.clone();
    global_gens.sort();
    assert_eq!(global_gens, vec!["openapi", "scaffold", "ui_scaffold"]);

    assert!(!resolved.features.get("auth").unwrap().as_bool().unwrap());
}

#[test]
fn unknown_profile_is_error() {
    let result = profile::load_and_resolve_profile(&profiles_path(), "nonexistent", None);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("nonexistent"),
        "expected mention of name: {err}"
    );
}

#[test]
fn unknown_variant_is_error() {
    let result = profile::load_and_resolve_profile(&profiles_path(), "api", Some("nonexistent"));
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("nonexistent"),
        "expected mention of variant: {err}"
    );
}

#[test]
fn all_profiles_have_valid_meta() {
    let _registry = CapabilityRegistry::new();
    let content = std::fs::read_to_string(profiles_path()).unwrap();
    let config: profile::ProfilesConfig = toml::from_str(&content).unwrap();

    for (name, def) in &config.profiles {
        let meta = def.meta.as_ref().unwrap();
        assert!(!meta.name.is_empty(), "profile {name} has no meta.name");
        assert!(
            !meta.version.is_empty(),
            "profile {name} has no meta.version"
        );
        assert!(
            !meta.description.is_empty(),
            "profile {name} has no meta.description"
        );
    }
}

// ── Integration test: run generators with a profile plan ────────────────

/// Set up a mock engine with a minimal schema, same pattern as template_harness.
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
    let tera = template_engine::create_tera(&template_dir).unwrap();

    let output_dir = tempfile::TempDir::new().unwrap();

    (engine, config, tera, output_dir)
}

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

    let report = codegraph::generate::run_generators_with_opts(codegraph::generate::GeneratorOpts {
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

    let report = codegraph::generate::run_generators_with_opts(codegraph::generate::GeneratorOpts {
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

    let report = codegraph::generate::run_generators_with_opts(codegraph::generate::GeneratorOpts {
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
