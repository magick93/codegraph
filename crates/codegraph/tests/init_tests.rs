//! Project-init lifecycle integration tests: `init`, `doctor`, `add domain`,
//! and the embedded rev accessor. All hermetic (tempfile, no network).

use std::fs;
use std::path::{Path, PathBuf};

use codegraph::init::commands::{cmd_add_domain, cmd_doctor, cmd_init, DoctorArgs, InitArgs};
use codegraph::init::{ProjectFeatures, ProjectTemplateContext};
use codegraph::profile::{load_and_resolve_profile, BuildPlan, CapabilityRegistry};
use tempfile::TempDir;

/// Absolute repo root (`<repo>/crates/codegraph` → two parents up).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("codegraph crate lives at <repo>/crates/codegraph")
        .to_path_buf()
}

fn features() -> ProjectFeatures {
    ProjectFeatures {
        grpc: false,
        ifml: false,
        ops: true,
    }
}

fn init_args(dir: &Path, name: &str, codegraph_path: Option<PathBuf>, force: bool) -> InitArgs {
    InitArgs {
        name: Some(name.to_string()),
        output_dir: dir.to_path_buf(),
        domains: vec!["common".to_string()],
        database_target: "postgres".to_string(),
        persistence_provider: "sea_orm".to_string(),
        deployment_topology: "monolith".to_string(),
        grpc: false,
        ifml: false,
        ops: true,
        rev: Some("abc123".to_string()),
        codegraph_path,
        force,
        template_dirs: vec![],
    }
}

fn assert_scaffold_files_exist(project: &Path) {
    let ctx = ProjectTemplateContext::new(
        "demo-app",
        &["common".to_string()],
        "abc123",
        None,
        "postgres",
        "sea_orm",
        "monolith",
        features(),
    );
    let expected = ctx.file_tree();
    assert_eq!(expected.len(), 16, "PROJECT_TEMPLATES should list 16 files");
    for rel in &expected {
        assert!(project.join(rel).is_file(), "missing {}", rel.display());
    }
    let mut actual: Vec<PathBuf> = walkdir::WalkDir::new(project)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().to_path_buf())
        .map(|p| p.strip_prefix(project).unwrap().to_path_buf())
        .collect();
    actual.sort();
    assert_eq!(actual.len(), 16, "unexpected extra files: {actual:?}");
}

#[test]
fn init_scaffolds_expected_file_tree() {
    let dir = TempDir::new().unwrap();
    let root = repo_root().canonicalize().unwrap();
    cmd_init(&init_args(dir.path(), "demo-app", Some(root.clone()), false)).unwrap();

    let project = dir.path().join("demo-app");
    assert_scaffold_files_exist(&project);

    let domains =
        codegraph_config::config::parse_domain_config(&project.join("domains.toml")).unwrap();
    assert!(domains.domains.contains_key("common"));

    let resolved =
        load_and_resolve_profile(&project.join("profiles.toml"), "default", None).unwrap();
    let plan = BuildPlan::from_profile(&resolved, &CapabilityRegistry::new()).unwrap();
    assert!(!plan.entity_generators.is_empty());

    codegraph_config::ops_manifest::OpsManifest::load(&project.join("codegraph-ops.toml"))
        .unwrap();

    codegraph_classifier::config::parse_classifier_config(&project.join("classifier.toml"))
        .unwrap();

    let schema = fs::read_to_string(project.join("schemas/common/example.json")).unwrap();
    serde_json::from_str::<serde_json::Value>(&schema).unwrap();

    let workspace = fs::read_to_string(project.join("Cargo.toml")).unwrap();
    let expected_path = format!("path = \"{}\"", root.join("crates/codegraph").display());
    assert!(
        workspace.contains(&expected_path),
        "workspace Cargo.toml should pin the codegraph crate via path:\n{workspace}"
    );
    assert!(
        !workspace.contains("magick93"),
        "path mode must not reference the git repo:\n{workspace}"
    );

    let main = fs::read_to_string(project.join("demo-app-graph/src/main.rs")).unwrap();
    assert!(
        main.contains("CODEGRAPH_REV: &str = \"abc123\""),
        "wrapper should stamp the rev:\n{main}"
    );
}

#[test]
fn init_git_rev_mode_pins_rev_in_workspace() {
    let dir = TempDir::new().unwrap();
    cmd_init(&init_args(dir.path(), "demo-app", None, false)).unwrap();

    let workspace = fs::read_to_string(dir.path().join("demo-app/Cargo.toml")).unwrap();
    let git_pins = workspace
        .matches("git = \"https://github.com/magick93/codegraph.git\"")
        .count();
    let rev_pins = workspace.matches("rev = \"abc123\"").count();
    assert_eq!(
        git_pins, 11,
        "all 11 codegraph workspace deps should pin the git repo:\n{workspace}"
    );
    assert_eq!(
        rev_pins, 11,
        "all 11 codegraph workspace deps should pin rev abc123:\n{workspace}"
    );

    let main = fs::read_to_string(dir.path().join("demo-app/demo-app-graph/src/main.rs")).unwrap();
    assert!(
        main.contains("CODEGRAPH_REV: &str = \"abc123\""),
        "wrapper should stamp the rev:\n{main}"
    );
}

#[test]
fn init_refuses_overwrite_without_force() {
    let dir = TempDir::new().unwrap();
    cmd_init(&init_args(dir.path(), "demo-app", None, false)).unwrap();
    let err = cmd_init(&init_args(dir.path(), "demo-app", None, false)).unwrap_err();
    assert!(
        format!("{err}").contains("force"),
        "overwrite refusal should mention --force: {err}"
    );
}

#[test]
fn init_force_overwrites() {
    let dir = TempDir::new().unwrap();
    cmd_init(&init_args(dir.path(), "demo-app", None, false)).unwrap();
    cmd_init(&init_args(dir.path(), "demo-app", None, true)).unwrap();
    assert!(dir.path().join("demo-app/Cargo.toml").is_file());
}

#[test]
fn init_normalizes_project_name() {
    let dir = TempDir::new().unwrap();
    cmd_init(&init_args(dir.path(), "My Cool App", None, false)).unwrap();
    assert!(dir.path().join("my-cool-app/Cargo.toml").is_file());
    assert!(!dir.path().join("My Cool App").exists());
}

#[test]
fn doctor_passes_on_scaffolded_project() {
    let dir = TempDir::new().unwrap();
    cmd_init(&init_args(dir.path(), "demo-app", Some(repo_root()), false)).unwrap();
    let project = dir.path().join("demo-app");

    cmd_doctor(&DoctorArgs {
        config: project.join("domains.toml"),
        schemas: project.join("schemas"),
        classifier: project.join("classifier.toml"),
        profiles_config: Some(project.join("profiles.toml")),
    })
    .unwrap();
}

#[test]
fn doctor_fails_on_missing_schemas() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("domains.toml");
    let classifier = dir.path().join("classifier.toml");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    fs::copy(fixtures.join("domains.toml"), &config).unwrap();
    fs::copy(fixtures.join("classifier.toml"), &classifier).unwrap();

    let err = cmd_doctor(&DoctorArgs {
        config,
        schemas: dir.path().join("schemas"),
        classifier,
        profiles_config: None,
    })
    .unwrap_err();
    assert!(
        format!("{err}").contains("hard check"),
        "missing schemas dir should be a hard failure: {err}"
    );
}

fn copy_fixture_domains(dir: &Path) -> PathBuf {
    let config = dir.join("domains.toml");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/domains.toml");
    fs::copy(fixture, &config).unwrap();
    config
}

#[test]
fn add_domain_appends_and_creates_schema() {
    let dir = TempDir::new().unwrap();
    let config = copy_fixture_domains(dir.path());
    let schemas = dir.path().join("schemas");

    cmd_add_domain(&config, &schemas, "billing").unwrap();

    let parsed = codegraph_config::config::parse_domain_config(&config).unwrap();
    assert!(parsed.domains.contains_key("billing"));
    assert_eq!(parsed.domains["billing"].label, "Billing");

    let example = schemas.join("billing/example.json");
    assert!(example.is_file());
    let json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&example).unwrap()).unwrap();
    assert!(json.is_object());
}

#[test]
fn add_domain_rejects_duplicate() {
    let dir = TempDir::new().unwrap();
    let config = copy_fixture_domains(dir.path());
    let schemas = dir.path().join("schemas");

    cmd_add_domain(&config, &schemas, "billing").unwrap();
    let err = cmd_add_domain(&config, &schemas, "billing").unwrap_err();
    assert!(
        format!("{err}").contains("already exists"),
        "duplicate domain should be rejected: {err}"
    );
}

#[test]
fn add_domain_normalizes_name() {
    let dir = TempDir::new().unwrap();
    let config = copy_fixture_domains(dir.path());
    let schemas = dir.path().join("schemas");

    cmd_add_domain(&config, &schemas, "Billing Accounts").unwrap();

    let parsed = codegraph_config::config::parse_domain_config(&config).unwrap();
    assert!(parsed.domains.contains_key("billing_accounts"));
    assert!(schemas.join("billing_accounts/example.json").is_file());
}

#[test]
fn rev_accessor_is_hex_or_empty() {
    let rev = codegraph::rev::codegraph_rev();
    if !rev.is_empty() {
        assert_eq!(rev.len(), 40, "git SHAs are 40 hex chars: {rev}");
        assert!(
            rev.chars().all(|c| c.is_ascii_hexdigit()),
            "rev should be hex: {rev}"
        );
    }
}
