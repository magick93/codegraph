//! Cross-run generation determinism gate.
//!
//! Rust `HashMap`/`HashSet` iteration order is randomized per thread (and per
//! process). Any generator that lets set/map iteration order leak into emitted
//! files produces byte-different output between runs — which breaks snapshot
//! reviews, `git diff` hygiene on generated output, and downstream byte-diff
//! verification. This test runs the full fixture pipeline on two OS threads
//! (guaranteed distinct `RandomState` seeds) and requires byte-identical
//! output trees.
//!
//! Emission boundaries that feed generated files must sort their data (see
//! `resolve_filter_fields`, `resolve_nested_filter_fields`, the handler
//! topology children, and the workflow-seed timers for the pattern).

#[path = "test_framework/mod.rs"]
mod test_framework;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Recursively collect `relative path -> bytes` for every file under `root`.
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

/// Run the full fixture pipeline into `output_dir` (mirrors
/// `grafeo_e2e_tests::generate_full_app` minus the gRPC generators).
async fn run_generation(output_dir: &Path) {
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
        let engine = codegraph_grafeo::GrafeoEngine::in_memory().unwrap();
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
        (engine, config)
    };

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
    let profiles_path = workspace_root.join("profiles.toml");

    let registry = codegraph::profile::CapabilityRegistry::new();
    let mut resolved =
        codegraph::profile::load_and_resolve_profile(&profiles_path, "default", None).unwrap();
    for section in resolved.sections.values_mut() {
        section.generators.retain(|g| !g.starts_with("grpc_"));
    }
    let plan = codegraph::profile::BuildPlan::from_profile(&resolved, &registry).unwrap();

    let domain_types_dir = output_dir.join("domain-types");
    let hooks_tmp = tempfile::TempDir::new().unwrap();

    let project_config = codegraph::generate::ProjectConfig {
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

    let ui_overrides = codegraph_config::UiOverrideConfig::default();
    let ui_domains = codegraph_config::UiDomainConfig::default();
    let report =
        codegraph::generate::run_generators_with_opts(codegraph::generate::GeneratorOpts {
            db: &engine,
            config: &config,
            output_dir,
            tera: &tera,
            ui_overrides: &ui_overrides,
            ui_domains: &ui_domains,
            schema_base_dir: Path::new(""),
            seed_config: None,
            domain_types_base: Some(&domain_types_dir),
            hooks_base: Some(hooks_tmp.path()),
            ext_points: None,
            build_plan: Some(&plan),
            ifml_frameworks: vec![],
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
}

#[tokio::test(flavor = "multi_thread")]
async fn two_thread_generation_is_byte_identical() {
    let dir_a = tempfile::TempDir::new().unwrap();
    let dir_b = tempfile::TempDir::new().unwrap();
    let path_a = dir_a.path().to_path_buf();
    let path_b = dir_b.path().to_path_buf();

    // Distinct OS threads => distinct `RandomState` seeds => distinct
    // HashMap/HashSet iteration orders between the two runs.
    let (res_a, res_b) = tokio::join!(
        tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(run_generation(&path_a))
        }),
        tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(run_generation(&path_b))
        }),
    );
    res_a.unwrap();
    res_b.unwrap();

    let tree_a = collect_tree(dir_a.path());
    let tree_b = collect_tree(dir_b.path());

    let a_names: Vec<_> = tree_a.keys().cloned().collect();
    let b_names: Vec<_> = tree_b.keys().cloned().collect();
    assert_eq!(a_names, b_names, "generated file sets differ between runs");

    let a_str = dir_a.path().to_string_lossy().into_owned();
    let b_str = dir_b.path().to_string_lossy().into_owned();
    let mut diffs: Vec<String> = Vec::new();
    for (name, bytes_a) in &tree_a {
        let bytes_b = &tree_b[name];
        // Manifests embed the absolute output path; normalize it before
        // comparing so only genuine content differences count.
        let norm_a = bytes_a.clone();
        let norm_b = bytes_b.clone();
        let text_a = String::from_utf8_lossy(&norm_a).replace(&a_str, "OUT");
        let text_b = String::from_utf8_lossy(&norm_b).replace(&b_str, "OUT");
        if text_a != text_b {
            diffs.push(name.clone());
        }
    }
    assert!(
        diffs.is_empty(),
        "generation is not deterministic — {} files differ between two runs:\n{}",
        diffs.len(),
        diffs
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
