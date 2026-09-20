//! Project-init lifecycle integration tests: `init`, `doctor`, `add domain`,
//! and the embedded rev accessor. All hermetic (tempfile, no network).
//!
//! Init is mox-first (issue #231): the scaffold emits `model/<domain>.mox`
//! per domain and NO `schemas/` directory or `classifier.toml`.

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

fn assert_scaffold_files_exist(project: &Path, domains: &[&str]) {
    let domain_names: Vec<String> = domains.iter().map(|d| d.to_string()).collect();
    let ctx = ProjectTemplateContext::new(
        "demo-app",
        &domain_names,
        "abc123",
        None,
        "postgres",
        "sea_orm",
        "monolith",
        features(),
    );
    let mut expected = ctx.file_tree();
    // 15 template entries minus the per-domain model expansion: 14 fixed
    // outputs + one model/<domain>.mox per domain.
    assert_eq!(
        expected.len(),
        14 + domains.len(),
        "file tree should be 14 fixed outputs + one model file per domain"
    );
    expected.sort();
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
    assert_eq!(actual, expected, "unexpected extra files: {actual:?}");
}

fn assert_mox_first_layout(project: &Path) {
    assert!(
        !project.join("schemas").exists(),
        "mox-first scaffold must not create a schemas/ directory"
    );
    assert!(
        !project.join("classifier.toml").exists(),
        "mox-first scaffold must not create classifier.toml"
    );
}

fn assert_starter_model_compiles(project: &Path, domain: &str) {
    let path = project.join("model").join(format!("{domain}.mox"));
    let content = fs::read_to_string(&path).unwrap();
    let compilation = rex_driver::compile_files(&[(path.display().to_string(), content.clone())]);
    assert!(
        compilation.model.is_some(),
        "starter model {domain}.mox must compile: {:?}",
        compilation.diagnostics
    );
    let model = compilation.model.unwrap();
    assert_eq!(
        model.packages[0].name, domain,
        "package name must match the domain"
    );
    let classes: Vec<&str> = model.packages[0]
        .classes
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert!(
        classes.contains(&"TodoListType") && classes.contains(&"TodoItemType"),
        "starter model must carry the TODO starter classes, got {classes:?}"
    );
}

#[test]
fn init_scaffolds_expected_file_tree() {
    let dir = TempDir::new().unwrap();
    let root = repo_root().canonicalize().unwrap();
    cmd_init(&init_args(
        dir.path(),
        "demo-app",
        Some(root.clone()),
        false,
    ))
    .unwrap();

    let project = dir.path().join("demo-app");
    assert_scaffold_files_exist(&project, &["common"]);
    assert_mox_first_layout(&project);

    let domains =
        codegraph_config::config::parse_domain_config(&project.join("domains.toml")).unwrap();
    assert!(domains.domains.contains_key("common"));

    let resolved =
        load_and_resolve_profile(&project.join("profiles.toml"), "default", None).unwrap();
    let plan = BuildPlan::from_profile(&resolved, &CapabilityRegistry::new()).unwrap();
    assert!(!plan.entity_generators.is_empty());

    assert_starter_model_compiles(&project, "common");

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
fn init_scaffolds_one_model_file_per_domain() {
    let dir = TempDir::new().unwrap();
    let mut args = init_args(dir.path(), "demo-app", None, false);
    args.domains = vec!["common".to_string(), "billing".to_string()];
    cmd_init(&args).unwrap();

    let project = dir.path().join("demo-app");
    assert_scaffold_files_exist(&project, &["common", "billing"]);
    assert_mox_first_layout(&project);
    assert_starter_model_compiles(&project, "common");
    assert_starter_model_compiles(&project, "billing");

    let domains =
        codegraph_config::config::parse_domain_config(&project.join("domains.toml")).unwrap();
    assert!(domains.domains.contains_key("common"));
    assert!(domains.domains.contains_key("billing"));
}

/// The generated domains.toml must not pin `entities` — mox is
/// author-declarative and the bridge decides entity vs VO.
#[test]
fn init_domains_toml_has_no_entities_key() {
    let dir = TempDir::new().unwrap();
    cmd_init(&init_args(dir.path(), "demo-app", None, false)).unwrap();

    let raw = fs::read_to_string(dir.path().join("demo-app/domains.toml")).unwrap();
    assert!(
        !raw.contains("entities"),
        "domains.toml must not carry an entities key:\n{raw}"
    );
}

/// The generated wrapper + justfile must be mox-first: Run/Classify/Doctor
/// take --mox-files and no longer default --schemas/--classifier.
#[test]
fn init_wrapper_and_justfile_are_mox_first() {
    let dir = TempDir::new().unwrap();
    cmd_init(&init_args(dir.path(), "demo-app", None, false)).unwrap();
    let project = dir.path().join("demo-app");

    let main = fs::read_to_string(project.join("demo-app-graph/src/main.rs")).unwrap();
    assert!(
        !main.contains("default_value = \"schemas\""),
        "wrapper Run must not default --schemas:\n{main}"
    );
    assert!(
        !main.contains("default_value = \"classifier.toml\""),
        "wrapper must not default --classifier:\n{main}"
    );
    assert!(
        main.contains("mox_files"),
        "wrapper must accept --mox-files:\n{main}"
    );

    let justfile = fs::read_to_string(project.join("justfile")).unwrap();
    assert!(
        justfile.contains("--mox-files model/common.mox"),
        "justfile recipes must pass --mox-files per domain:\n{justfile}"
    );
    assert!(
        !justfile.contains("--classifier"),
        "justfile must not reference --classifier:\n{justfile}"
    );
    assert!(
        justfile.contains("Regenerate code from your .mox model"),
        "justfile generate comment must reference the .mox model:\n{justfile}"
    );

    let domains_toml = fs::read_to_string(project.join("domains.toml")).unwrap();
    assert!(
        domains_toml.contains("model/common.mox"),
        "domains.toml should point at the model file:\n{domains_toml}"
    );

    let readme = fs::read_to_string(project.join("README.md")).unwrap();
    assert!(
        readme.contains(".mox model"),
        "README should describe the .mox-first scaffold:\n{readme}"
    );
    assert!(
        !readme.contains("classifier.toml"),
        "README layout must not list classifier.toml:\n{readme}"
    );
}

/// The ops manifest emitted by init must be mox-first: `mox_files` set, no
/// `schemas_dir`/`classifier` keys, and it must load through the harness's
/// `OpsConfig` (contract from PR #250).
#[test]
fn init_ops_manifest_is_mox_first() {
    let dir = TempDir::new().unwrap();
    let mut args = init_args(dir.path(), "demo-app", None, false);
    args.domains = vec!["common".to_string(), "billing".to_string()];
    cmd_init(&args).unwrap();

    let project = dir.path().join("demo-app");
    let raw = fs::read_to_string(project.join("codegraph-ops.toml")).unwrap();
    assert!(
        raw.contains(r#"mox_files = ["model/common.mox", "model/billing.mox"]"#),
        "manifest must list one mox file per domain in domain order:\n{raw}"
    );
    assert!(
        !raw.contains("schemas_dir"),
        "manifest must not carry schemas_dir:\n{raw}"
    );
    assert!(
        !raw.contains("classifier"),
        "manifest must not carry classifier:\n{raw}"
    );

    let manifest_path = project.join("codegraph-ops.toml");
    let cfg = codegraph_ops::OpsConfig::load(&manifest_path)
        .expect("emitted manifest must load via OpsConfig::load");
    assert_eq!(
        cfg.manifest.mox_files,
        vec![
            "model/common.mox".to_string(),
            "model/billing.mox".to_string()
        ]
    );
}

/// THE acceptance gate: a fresh scaffold must generate with ONLY --mox-files
/// + --config (no --schemas, no --classifier), producing DDL for both starter
/// classes with the refers-derived FK.
#[tokio::test]
async fn init_scaffold_runs_mox_first() {
    let dir = TempDir::new().unwrap();
    let root = repo_root().canonicalize().unwrap();
    cmd_init(&init_args(dir.path(), "demo-app", Some(root), false)).unwrap();
    let project = dir.path().join("demo-app");

    let mox_files = vec![project.join("model/common.mox")];
    let output = project.join("generated");
    codegraph::driver::run(codegraph::driver::RunArgs {
        schemas: None,
        classifier: None,
        config_path: &project.join("domains.toml"),
        output: &output,
        extension_points_path: None,
        profile_name: "default",
        variant: None,
        profiles_config_path: Some(project.join("profiles.toml")),
        no_post_gen: true,
        template_dir: &[],
        ifml_files: &[],
        openapi_files: &[],
        mox_files: &mox_files,
        ifml_framework: &[],
        ifml_components: None,
        ifml_design_system: None,
        codegraph_rev: None,
    })
    .await
    .unwrap();

    let generated: Vec<PathBuf> = walkdir::WalkDir::new(&output)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().to_path_buf())
        .collect();
    assert!(
        !generated.is_empty(),
        "mox-first generation must produce files"
    );

    let migrations_dir = output.join("migrations");
    let mut ddl = String::new();
    for entry in fs::read_dir(&migrations_dir).unwrap_or_else(|e| {
        panic!(
            "migrations dir missing under {}: {e}",
            migrations_dir.display()
        )
    }) {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) == Some("sql") {
            ddl.push_str(&fs::read_to_string(&path).unwrap());
            ddl.push('\n');
        }
    }
    assert!(
        ddl.contains("todo_list"),
        "DDL must contain todo_list:\n{ddl}"
    );
    assert!(
        ddl.contains("todo_item"),
        "DDL must contain todo_item:\n{ddl}"
    );
    assert!(
        ddl.contains("todo_list_id"),
        "starter refers must produce a todo_list_id FK on todo_item:\n{ddl}"
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

/// Doctor with the wrapper's new default args (mox files set, no
/// --schemas/--classifier) must pass on a fresh scaffold with ZERO model
/// warnings — the intentional new-project shape is warning-free.
#[test]
fn doctor_fresh_scaffold_has_zero_model_warnings() {
    let dir = TempDir::new().unwrap();
    cmd_init(&init_args(dir.path(), "demo-app", Some(repo_root()), false)).unwrap();
    let project = dir.path().join("demo-app");

    let summary = cmd_doctor(&DoctorArgs {
        config: project.join("domains.toml"),
        schemas: None,
        classifier: None,
        profiles_config: Some(project.join("profiles.toml")),
        mox_files: vec![project.join("model/common.mox")],
    })
    .unwrap();
    assert_eq!(
        summary.hard_failures, 0,
        "fresh scaffold must have no hard failures"
    );
    assert_eq!(
        summary.model_warnings, 0,
        "fresh scaffold doctor must produce zero model warnings"
    );
}

/// A schemas directory that exists but is empty is still a misconfiguration
/// in mox mode — the WARN stays.
#[test]
fn doctor_empty_schemas_dir_in_mox_mode_still_warns() {
    let dir = TempDir::new().unwrap();
    let config = copy_doctor_fixture_files(&dir);
    let schemas = dir.path().join("schemas");
    fs::create_dir_all(&schemas).unwrap();
    let mox = dir.path().join("model.mox");
    fs::write(&mox, DOCTOR_MOX).unwrap();

    let summary = cmd_doctor(&DoctorArgs {
        config,
        schemas: Some(schemas),
        classifier: None,
        profiles_config: None,
        mox_files: vec![mox],
    })
    .unwrap();
    assert_eq!(
        summary.model_warnings, 1,
        "empty schemas dir in mox mode must still warn"
    );
}

/// classifier.toml is only required when JSON schemas are present: absent
/// classifier + a schemas dir with JSON is a hard failure.
#[test]
fn doctor_classifier_missing_with_json_schemas_is_hard_failure() {
    let dir = TempDir::new().unwrap();
    let config = copy_fixture_domains(dir.path());
    let schemas = dir.path().join("schemas/common");
    fs::create_dir_all(&schemas).unwrap();
    fs::write(
        schemas.parent().unwrap().join("common/Widget.json"),
        r#"{ "title": "Widget", "type": "object" }"#,
    )
    .unwrap();

    let err = cmd_doctor(&DoctorArgs {
        config,
        schemas: Some(dir.path().join("schemas")),
        classifier: None,
        profiles_config: None,
        mox_files: vec![],
    })
    .unwrap_err();
    assert!(
        format!("{err}").contains("hard check"),
        "classifier missing with JSON schemas present should be a hard failure: {err}"
    );
}

/// Doctor's mox compile must accept MULTIPLE --mox-files (one per domain).
#[test]
fn doctor_validates_multiple_mox_files() {
    let dir = TempDir::new().unwrap();
    let mut args = init_args(dir.path(), "demo-app", None, false);
    args.domains = vec!["common".to_string(), "billing".to_string()];
    cmd_init(&args).unwrap();
    let project = dir.path().join("demo-app");

    let summary = cmd_doctor(&DoctorArgs {
        config: project.join("domains.toml"),
        schemas: None,
        classifier: None,
        profiles_config: None,
        mox_files: vec![
            project.join("model/common.mox"),
            project.join("model/billing.mox"),
        ],
    })
    .unwrap();
    assert_eq!(summary.hard_failures, 0);
    assert_eq!(summary.model_warnings, 0);
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
        schemas: Some(dir.path().join("schemas")),
        classifier: Some(classifier),
        profiles_config: None,
        mox_files: vec![],
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

const DOCTOR_MOX: &str = "package recruiting\n\nclass CandidateType {\n    String name\n}\n";

fn copy_doctor_fixture_files(dir: &TempDir) -> PathBuf {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    fs::copy(
        fixtures.join("classifier.toml"),
        dir.path().join("classifier.toml"),
    )
    .unwrap();
    copy_fixture_domains(dir.path())
}

#[test]
fn doctor_mox_mode_validates_packages_and_allows_missing_schemas() {
    let dir = TempDir::new().unwrap();
    let config = copy_doctor_fixture_files(&dir);
    let mox = dir.path().join("model.mox");
    fs::write(&mox, DOCTOR_MOX).unwrap();

    // The schemas dir does not exist; in mox mode that is the intentional
    // new-project shape (info, no warning); the compiling package matches
    // the fixture's recruiting domain.
    let summary = cmd_doctor(&DoctorArgs {
        config,
        schemas: Some(dir.path().join("schemas")),
        classifier: Some(dir.path().join("classifier.toml")),
        profiles_config: None,
        mox_files: vec![mox],
    })
    .unwrap();
    assert_eq!(summary.hard_failures, 0);
}

#[test]
fn doctor_mox_package_without_domain_entry_is_a_hard_failure() {
    let dir = TempDir::new().unwrap();
    let config = copy_doctor_fixture_files(&dir);
    let mox = dir.path().join("model.mox");
    fs::write(
        &mox,
        "package unknown_package\n\nclass Widget {\n    String name\n}\n",
    )
    .unwrap();

    let err = cmd_doctor(&DoctorArgs {
        config,
        schemas: Some(dir.path().join("schemas")),
        classifier: Some(dir.path().join("classifier.toml")),
        profiles_config: None,
        mox_files: vec![mox],
    })
    .unwrap_err();
    assert!(
        format!("{err}").contains("hard check"),
        "unmatched mox package should be a hard failure: {err}"
    );
}

#[test]
fn doctor_broken_mox_file_is_a_hard_failure() {
    let dir = TempDir::new().unwrap();
    let config = copy_doctor_fixture_files(&dir);
    let mox = dir.path().join("broken.mox");
    fs::write(
        &mox,
        "package recruiting\n\nclass Broken {\n    String\n}\n",
    )
    .unwrap();

    let err = cmd_doctor(&DoctorArgs {
        config,
        schemas: Some(dir.path().join("schemas")),
        classifier: Some(dir.path().join("classifier.toml")),
        profiles_config: None,
        mox_files: vec![mox],
    })
    .unwrap_err();
    assert!(
        format!("{err}").contains("hard check"),
        "broken mox should be a hard failure: {err}"
    );
}

#[test]
fn add_domain_appends_and_creates_mox_starter() {
    let dir = TempDir::new().unwrap();
    let config = copy_fixture_domains(dir.path());

    cmd_add_domain(&config, "billing").unwrap();

    let parsed = codegraph_config::config::parse_domain_config(&config).unwrap();
    assert!(parsed.domains.contains_key("billing"));
    assert_eq!(parsed.domains["billing"].label, "Billing");
    assert_eq!(parsed.domains["billing"].schema_dir, "billing");
    assert_eq!(parsed.domains["billing"].postgres_schema, "billing");

    let model = dir.path().join("model/billing.mox");
    assert!(model.is_file(), "add domain must create model/billing.mox");
    let content = fs::read_to_string(&model).unwrap();
    let compilation = rex_driver::compile_files(&[("model/billing.mox".to_string(), content)]);
    assert!(
        compilation.model.is_some(),
        "added domain starter must compile: {:?}",
        compilation.diagnostics
    );
    assert_eq!(compilation.model.unwrap().packages[0].name, "billing");

    assert!(
        !dir.path().join("schemas/billing").exists(),
        "add domain must not create a schemas/ directory"
    );

    let err = cmd_add_domain(&config, "billing").unwrap_err();
    assert!(
        format!("{err}").contains("already exists"),
        "duplicate domain should be rejected: {err}"
    );
}

#[test]
fn add_domain_rejects_duplicate() {
    let dir = TempDir::new().unwrap();
    let config = copy_fixture_domains(dir.path());

    cmd_add_domain(&config, "billing").unwrap();
    let err = cmd_add_domain(&config, "billing").unwrap_err();
    assert!(
        format!("{err}").contains("already exists"),
        "duplicate domain should be rejected: {err}"
    );
}

#[test]
fn add_domain_normalizes_name() {
    let dir = TempDir::new().unwrap();
    let config = copy_fixture_domains(dir.path());

    cmd_add_domain(&config, "Billing Accounts").unwrap();

    let parsed = codegraph_config::config::parse_domain_config(&config).unwrap();
    assert!(parsed.domains.contains_key("billing_accounts"));
    let model = dir.path().join("model/billing_accounts.mox");
    assert!(model.is_file());
    let content = fs::read_to_string(&model).unwrap();
    let compilation =
        rex_driver::compile_files(&[("model/billing_accounts.mox".to_string(), content)]);
    assert!(
        compilation.model.is_some(),
        "normalized domain starter must compile: {:?}",
        compilation.diagnostics
    );
    assert_eq!(
        compilation.model.unwrap().packages[0].name,
        "billing_accounts"
    );
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

// ── doctor: mox `import schema` validation (issue #230) ──

fn write_doctor_import_fixture(dir: &TempDir, mox_body: &str, json: Option<&str>) {
    let config = copy_doctor_fixture_files(dir);
    assert!(config.is_file());
    if let Some(json) = json {
        let schema = dir.path().join("schemas/item.json");
        fs::create_dir_all(schema.parent().unwrap()).unwrap();
        fs::write(schema, json).unwrap();
    }
    let mox = dir.path().join("model.mox");
    fs::write(&mox, mox_body).unwrap();
}

const IMPORT_MOX: &str = "package recruiting\n\nimport schema \"schemas/item.json\" as Item\n\nclass CandidateType {\n    String name\n}\n";

#[test]
fn doctor_valid_import_passes() {
    let dir = TempDir::new().unwrap();
    write_doctor_import_fixture(
        &dir,
        IMPORT_MOX,
        Some(r#"{ "title": "ItemType", "type": "object" }"#),
    );

    cmd_doctor(&DoctorArgs {
        config: dir.path().join("domains.toml"),
        schemas: Some(dir.path().join("schemas")),
        classifier: Some(dir.path().join("classifier.toml")),
        profiles_config: None,
        mox_files: vec![dir.path().join("model.mox")],
    })
    .unwrap();
}

#[test]
fn doctor_missing_import_target_is_a_hard_failure() {
    let dir = TempDir::new().unwrap();
    write_doctor_import_fixture(&dir, IMPORT_MOX, None);

    let err = cmd_doctor(&DoctorArgs {
        config: dir.path().join("domains.toml"),
        schemas: Some(dir.path().join("schemas")),
        classifier: Some(dir.path().join("classifier.toml")),
        profiles_config: None,
        mox_files: vec![dir.path().join("model.mox")],
    })
    .unwrap_err();
    assert!(
        format!("{err}").contains("hard check"),
        "missing import target should be a hard failure: {err}"
    );
}

#[test]
fn doctor_invalid_import_json_is_a_hard_failure() {
    let dir = TempDir::new().unwrap();
    write_doctor_import_fixture(&dir, IMPORT_MOX, Some("{ not json"));

    let err = cmd_doctor(&DoctorArgs {
        config: dir.path().join("domains.toml"),
        schemas: Some(dir.path().join("schemas")),
        classifier: Some(dir.path().join("classifier.toml")),
        profiles_config: None,
        mox_files: vec![dir.path().join("model.mox")],
    })
    .unwrap_err();
    assert!(
        format!("{err}").contains("hard check"),
        "invalid import JSON should be a hard failure: {err}"
    );
}
