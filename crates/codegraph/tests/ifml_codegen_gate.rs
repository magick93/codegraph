//! IFML codegen validation gate (Wave A).
//!
//! Full-stack harness: runs the complete pipeline (schemas + classifier +
//! domains.toml + profiles.toml + IFML DSL) into a fixed gate root
//! (`target/ifml-gate/`), writes the gate-owned extras (vite `/api` proxy,
//! playwright config, ui stubs, #205 sweep spec), then drives four
//! validation stages:
//!
//! - T0 `gate_full_stack_boots`: migrations on a fresh Postgres DB, build and
//!   boot the generated axum server, wait for `/health`.
//! - T1 `generated_app_typechecks`: `svelte-check` over the generated pages.
//! - T2 `generated_app_builds`: `vite build` produces `dist/`.
//! - T3 `specs_pass_against_real_api`: generated Playwright specs against the
//!   app served via `vite preview` with `/api` proxied to the booted axum
//!   server, asserting zero failures and expected test categories.
//! - T4 `regen_after_view_removal_stays_green`: removes one view from the
//!   .ifml fixture, regenerates into the same root, reruns T1-T3 assertions.
//!
//! Every test is `#[ignore]`-gated (run with `cargo test -p codegraph --test
//! ifml_codegen_gate -- --ignored`) and auto-skips with a printed reason when
//! node/npx, a chromium install, or Postgres is unavailable.
//!
//! The reusable harness pieces (process helpers, npm project management,
//! Postgres provisioning, axum server build/boot, Playwright runs, and the
//! scaffolding writers) live in `tests/test_framework/`; this file keeps the
//! gate's fixture knowledge plus the test functions.
//!
//! The SvelteKit skeleton (package.json, tsconfig, app shell, …) is expected
//! from the GENERATOR (G1, Wave B); the gate no longer writes it. The vite
//! proxy, playwright config, ui stubs, and sweep spec stay gate-owned.
//! Generator-emitted files are never overwritten.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use codegraph_core::traits::GraphQuerier;

#[path = "test_framework/mod.rs"]
mod test_framework;

use test_framework::axum_server;
use test_framework::extras::write_extras;
use test_framework::node_project::NodeProject;
use test_framework::playwright;
use test_framework::postgres::{resolve_base_target, GateDb};
use test_framework::process::{free_port, have_tool};

/// Serializes the whole gate so concurrent tests never race on the shared
/// gate root, the generated crate build lock, or the npm install directory.
static GATE_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

fn gate_lock() -> &'static tokio::sync::Mutex<()> {
    GATE_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

// ── Paths ────────────────────────────────────────────────────────────────────

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("codegraph crate lives at <repo>/crates/codegraph")
        .to_path_buf()
}

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ifml_gate")
}

/// Fixed gate root (inside target/, already gitignored) so cargo target
/// caching and node_modules persist across runs.
fn gate_root() -> PathBuf {
    repo_root().join("target").join("ifml-gate")
}

fn fixture_home() -> PathBuf {
    gate_root().join("fixture")
}

fn app_dir() -> PathBuf {
    gate_root().join("app")
}

fn svelte_dir() -> PathBuf {
    app_dir().join("svelte")
}

fn gate_target_dir() -> PathBuf {
    gate_root().join("cargo-target")
}

fn logs_dir() -> PathBuf {
    gate_root().join("logs")
}

fn svelte_project() -> NodeProject {
    NodeProject::new(svelte_dir(), logs_dir())
}

// ── Environment / skip detection ─────────────────────────────────────────────

fn skip(reason: &str) {
    println!("skip: {reason}");
}

// ── Fixture staging ──────────────────────────────────────────────────────────

/// Copy the pristine fixture into the gate fixture home (config home for the
/// pipeline: domains.toml, classifier.toml, profiles.toml, schemas/, app.ifml,
/// policy.actor, refunds.mox). The previous staging is removed first so moves
/// and deletions in the fixture are reflected (stale schema files would be
/// re-ingested).
fn stage_fixture() -> Result<(), String> {
    let src = fixture_dir();
    let dst = fixture_home();
    if dst.exists() {
        fs::remove_dir_all(&dst).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&dst).map_err(|e| e.to_string())?;
    copy_tree(&src, &dst)
}

fn copy_tree(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Remove one top-level `view "Name" { ... }` block from an .ifml file,
/// tracking brace depth so nested blocks stay intact.
fn remove_view_from_ifml(path: &Path, view: &str) -> Result<usize, String> {
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let marker = format!("view \"{view}\" {{");
    let Some(start) = content.find(&marker) else {
        return Err(format!("view {view} not found in {}", path.display()));
    };
    let bytes = content.as_bytes();
    let mut depth = 0usize;
    let mut end = start;
    while end < bytes.len() {
        match bytes[end] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    end += 1;
                    break;
                }
            }
            _ => {}
        }
        end += 1;
    }
    let mut out = String::with_capacity(content.len());
    out.push_str(&content[..start]);
    out.push_str(&content[end..]);
    let removed = content.len() - out.len();
    fs::write(path, out).map_err(|e| e.to_string())?;
    Ok(removed)
}

// ── Pipeline (codegraph run semantics, in-process) ───────────────────────────

/// Full pipeline: schemas + classifier + domains.toml + profiles.toml + IFML
/// into `target/ifml-gate/app`. Uses a fresh in-memory Grafeo engine per run.
async fn run_pipeline() -> Result<(), String> {
    let home = fixture_home();
    let output = app_dir();
    fs::create_dir_all(&output).map_err(|e| e.to_string())?;

    let config = codegraph_config::config::parse_domain_config(&home.join("domains.toml"))
        .map_err(|e| e.to_string())?;
    let classifier =
        codegraph_classifier::config::parse_classifier_config(&home.join("classifier.toml"))
            .map_err(|e| e.to_string())?;
    let registry = codegraph::profile::CapabilityRegistry::new();
    let resolved =
        codegraph::profile::load_and_resolve_profile(&home.join("profiles.toml"), "gate", None)
            .map_err(|e| e.to_string())?;
    let plan = codegraph::profile::BuildPlan::from_profile(&resolved, &registry)
        .map_err(|e| e.to_string())?;

    let engine = codegraph_grafeo::GrafeoEngine::in_memory().map_err(|e| e.to_string())?;

    let empty_entities = std::collections::HashSet::new();
    let ui_overrides = codegraph_config::UiOverrideConfig::default();
    codegraph::ingest::async_ingest::ingest_schemas(
        &engine,
        &home.join("schemas"),
        &classifier,
        &empty_entities,
        &ui_overrides,
        &config.defaults.type_suffix,
    )
    .await
    .map_err(|e| e.to_string())?;

    let ifml_path = home.join("app.ifml");
    let model = codegraph_ifml_dsl::parse_ifml_file(&ifml_path)
        .map_err(|e| format!("parse {}: {e}", ifml_path.display()))?;
    codegraph::ingest::ifml_ingest::ingest_ifml_model(&engine, &model)
        .await
        .map_err(|e| e.to_string())?;
    codegraph::ifml_actor_import::ingest_actor_imports(&engine, &engine, &model, &ifml_path)
        .await
        .map_err(|e| e.to_string())?;

    codegraph::ingest::api_ingest::ingest_api_model(&engine, &config)
        .await
        .map_err(|e| e.to_string())?;

    let classifier_types: std::collections::HashSet<String> = classifier
        .primitive_wrappers
        .keys()
        .cloned()
        .chain(classifier.array_wrappers.keys().cloned())
        .chain(classifier.range_wrappers.keys().cloned())
        .chain(
            classifier
                .composite_wrappers
                .iter()
                .map(|cw| cw.schema.clone()),
        )
        .collect();
    let all_data = engine
        .get_classification_data()
        .await
        .map_err(|e| e.to_string())?;
    let auto_classifier =
        codegraph::classify::AutoClassifier::new(classifier_types, classifier.naming_rules.clone());
    let mut entity_names = std::collections::HashSet::new();
    let mut sorted_domains: Vec<&String> = config.domains.keys().collect();
    sorted_domains.sort();
    for domain_name in &sorted_domains {
        let domain_entry = &config.domains[domain_name.as_str()];
        let domain_schemas: Vec<_> = all_data
            .iter()
            .filter(|d| d.domain.as_deref() == Some(domain_name.as_str()))
            .cloned()
            .collect();
        let result = auto_classifier.classify_domain(domain_name, domain_entry, &domain_schemas);
        for score in &result.entities {
            entity_names.insert(score.title.clone());
        }
    }
    for domain_entry in config.domains.values() {
        for entity in &domain_entry.entities {
            entity_names.insert(entity.clone());
        }
    }
    codegraph::ingest::async_ingest::reclassify_with_entities(&engine, &engine, &entity_names)
        .await
        .map_err(|e| e.to_string())?;

    let templates = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera =
        codegraph::generate::template_engine::create_tera(&templates).map_err(|e| e.to_string())?;

    let workspace = repo_root();
    let type_contracts = workspace.join("crates/codegraph-type-contracts");
    let workflow = workspace.join("crates/codegraph-workflow");
    let domain_types_dir = output.join("domain-types");

    let project_config = codegraph::generate::ProjectConfig {
        app_name: "ifml-gate-app".into(),
        lib_name: "cosmos".into(),
        domain_types_crate: "gate_domain_types".into(),
        api_title: "IFML Gate API".into(),
        generator_name: "codegraph-gate".into(),
        domain_types_base: "domain-types".into(),
        type_contracts_base: type_contracts.to_string_lossy().to_string(),
        codegraph_workflow_base: workflow.to_string_lossy().to_string(),
        database_target: "postgres".into(),
        persistence_provider: "sea_orm".into(),
        deployment_topology: "monolith".into(),
        api_version: config.defaults.api_version.clone(),
        types_import_prefix: config.defaults.types_import_prefix.clone(),
        codegraph_rev: String::new(),
        extra_dependencies: format!(
            "codegraph-workflow = {{ path = \"{}\" }}\n\
             codegraph-type-contracts = {{ path = \"{}\" }}",
            workflow.display(),
            type_contracts.display(),
        ),
        ..Default::default()
    };

    // Built-in shadcn-svelte pack, shadowed by the fixture's mapping
    // overrides (ifml-components.toml) so gate assertions can pin specific
    // wrapper components (issue #200: the Tabs presentation-container).
    let pack = codegraph_config::built_in_pack("shadcn-svelte").map_err(|e| e.to_string())?;
    let fixture_mappings_path = home.join("ifml-components.toml");
    let pack = if fixture_mappings_path.exists() {
        let project = codegraph_config::IfmlComponentMappings::load(&fixture_mappings_path)
            .map_err(|e| e.to_string())?;
        codegraph_config::IfmlComponentMappings::merge_with_pack(project, &pack)
    } else {
        pack
    };
    let hooks_tmp = tempfile::tempdir().map_err(|e| e.to_string())?;

    let report =
        codegraph::generate::run_generators_with_opts(codegraph::generate::GeneratorOpts {
            db: &engine,
            config: &config,
            output_dir: &output,
            tera: &tera,
            ui_overrides: &ui_overrides,
            ui_domains: &codegraph_config::UiDomainConfig::default(),
            schema_base_dir: &home.join("schemas"),
            seed_config: None,
            domain_types_base: Some(&domain_types_dir),
            hooks_base: Some(hooks_tmp.path()),
            ext_points: None,
            build_plan: Some(&plan),
            ifml_frameworks: vec!["svelte".to_string()],
            ifml_components: Some(&pack),
            project_config: Some(&project_config),
            emdash_plugins: None,
            domain_config_dir: Some(&home),
        })
        .await
        .map_err(|e| e.to_string())?;

    if report.has_errors() {
        return Err(format!(
            "generation reported errors: {:#?}",
            report
                .errors
                .iter()
                .map(|e| format!("{}/{}: {}", e.entity, e.generator, e.source))
                .collect::<Vec<_>>()
        ));
    }
    Ok(())
}

// ── Shared setup ─────────────────────────────────────────────────────────────

async fn ensure_generated() -> Result<(), String> {
    stage_fixture()?;
    run_pipeline().await?;
    write_extras(&svelte_dir())?;
    isolate_generated_workspace()
}

/// The generated app manifest must not claim membership of the codegraph
/// workspace (cargo refuses to build non-member packages inside the workspace
/// tree). Appending an empty `[workspace]` table makes the generated crate
/// its own workspace root — the same effect `codegraph init` achieves with
/// `exclude = ["generated"]`. Idempotent: the generator rewrites its manifest
/// on every run and the gate re-appends afterwards.
fn isolate_generated_workspace() -> Result<(), String> {
    let manifest = app_dir().join("Cargo.toml");
    let content = fs::read_to_string(&manifest).map_err(|e| e.to_string())?;
    if !content.contains("[workspace]") {
        fs::write(&manifest, format!("{content}\n[workspace]\n")).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ── Tests ────────────────────────────────────────────────────────────────────

/// T0: migrations on a fresh DB, build + boot the generated axum server,
/// wait for /health.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "full-stack gate: run with --ignored (needs node, chromium, postgres)"]
async fn gate_full_stack_boots() {
    let _guard = gate_lock().lock().await;
    if !have_tool("cargo") {
        skip("cargo not found");
        return;
    }
    let Ok(base) = resolve_base_target().await else {
        skip("no usable postgres (DATABASE_URL + docker fallback failed)");
        return;
    };

    ensure_generated().await.unwrap();

    let db = GateDb::create(base).await.unwrap();
    db.apply_prelude().await.unwrap();
    db.apply_migrations(&app_dir().join("migrations"))
        .await
        .unwrap();

    let result = async {
        let bin = axum_server::build_app(
            &app_dir().join("Cargo.toml"),
            &repo_root(),
            &gate_target_dir(),
            &logs_dir(),
            "ifml-gate-app",
        )?;
        let database_url = db.url();
        let server = axum_server::boot_server(
            &bin,
            &[
                ("DATABASE_URL", database_url.as_str()),
                ("SUPABASE_JWT_SECRET", "ifml-gate-test-secret"),
                ("APP_NAME", "ifml-gate-app"),
            ],
            "ifml-gate-app",
            &logs_dir(),
        )
        .await?;
        println!("gate: server booted on 127.0.0.1:{}", server.port);
        Ok::<(), String>(())
    }
    .await;

    db.cleanup().await;
    result.unwrap();
}

/// T1: the generated svelte app typechecks (svelte-check, zero errors).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "full-stack gate: run with --ignored (needs node)"]
async fn generated_app_typechecks() {
    let _guard = gate_lock().lock().await;
    if !have_tool("node") || !have_tool("npx") {
        skip("node/npx not found");
        return;
    }

    ensure_generated().await.unwrap();
    let project = svelte_project();
    project.ensure_install().unwrap();
    project.sync_sveltekit().unwrap();

    let Ok((ok, output)) = project.run_check() else {
        panic!("failed to launch svelte-check");
    };
    if !ok {
        panic!(
            "svelte-check reported errors (exit != 0). Output head:\n{}",
            output.lines().take(80).collect::<Vec<_>>().join("\n")
        );
    }
    let errors = output
        .lines()
        .filter(|l| l.contains("Error:") || l.starts_with("Error: "))
        .count();
    assert_eq!(
        errors, 0,
        "svelte-check reported {errors} errors:\n{output}"
    );
}

/// T2: the generated svelte app builds (vite build produces dist/).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "full-stack gate: run with --ignored (needs node)"]
async fn generated_app_builds() {
    let _guard = gate_lock().lock().await;
    if !have_tool("node") || !have_tool("npx") {
        skip("node/npx not found");
        return;
    }

    ensure_generated().await.unwrap();
    let project = svelte_project();
    project.ensure_install().unwrap();
    project.sync_sveltekit().unwrap();

    let ok = project.run_build().unwrap();
    assert!(ok, "vite build failed (see target/ifml-gate/logs)");
    // SvelteKit emits .svelte-kit/output/{client,server} rather than dist/.
    let svelte = svelte_dir();
    assert!(
        svelte.join(".svelte-kit/output/client").is_dir()
            && svelte.join(".svelte-kit/output/server").is_dir(),
        "vite build did not produce .svelte-kit/output artifacts"
    );
}

/// T3: the generated playwright specs pass against the real API.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "full-stack gate: run with --ignored (needs node, chromium, postgres)"]
async fn specs_pass_against_real_api() {
    let _guard = gate_lock().lock().await;
    if !have_tool("node") || !have_tool("npx") {
        skip("node/npx not found");
        return;
    }
    let Ok(base) = resolve_base_target().await else {
        skip("no usable postgres (DATABASE_URL + docker fallback failed)");
        return;
    };

    ensure_generated().await.unwrap();
    let project = svelte_project();
    project.ensure_install().unwrap();
    project.sync_sveltekit().unwrap();
    let Ok(()) = playwright::install_chromium(&project) else {
        skip("chromium not available and install failed");
        return;
    };

    let db = GateDb::create(base).await.unwrap();
    db.apply_prelude().await.unwrap();
    db.apply_migrations(&app_dir().join("migrations"))
        .await
        .unwrap();
    let api_key = db.provision_api_key().await.unwrap();

    let result = async {
        let bin = axum_server::build_app(
            &app_dir().join("Cargo.toml"),
            &repo_root(),
            &gate_target_dir(),
            &logs_dir(),
            "ifml-gate-app",
        )?;
        let database_url = db.url();
        let server = axum_server::boot_server(
            &bin,
            &[
                ("DATABASE_URL", database_url.as_str()),
                ("SUPABASE_JWT_SECRET", "ifml-gate-test-secret"),
                ("APP_NAME", "ifml-gate-app"),
            ],
            "ifml-gate-app",
            &logs_dir(),
        )
        .await?;
        let preview_port = free_port();
        let built = project.run_build()?;
        assert!(built, "vite build failed before the spec run");
        let report = playwright::run_specs(
            &project,
            &gate_root().join("playwright-report.json"),
            server.port,
            preview_port,
            &api_key,
        )?;
        println!(
            "gate: playwright report — passed {}, failed {}\n{}",
            report.passed,
            report.failed,
            report
                .titles
                .iter()
                .map(|t| format!("  - {t}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        assert_eq!(
            report.failed, 0,
            "{} generated spec(s) failed — see target/ifml-gate/playwright-report.json",
            report.failed
        );
        assert_categories(&report.titles);
        Ok::<(), String>(())
    }
    .await;

    db.cleanup().await;
    result.unwrap();
}

fn assert_categories(titles: &[String]) {
    let has = |pred: &dyn Fn(&str) -> bool| titles.iter().any(|t| pred(t));
    let render = has(&|t| t.starts_with("renders "));
    let persona_allow = has(&|t| {
        t.split_once(" views ")
            .map(|(actor, _)| !actor.is_empty())
            .unwrap_or(false)
            && t.starts_with("actor ")
    });
    let persona_deny = has(&|t| t.starts_with("actor ") && t.contains(" is redirected from "));
    let click_through = has(&|t| t.contains(" navigates to "));
    let validation = has(&|t| t == "form validation blocks empty submit");
    let round_trip = has(&|t| t == "form round trip persists changes");
    let workflow = has(&|t| t.starts_with("shows the initial workflow state for "));
    let workflow_transition = has(&|t| t.starts_with("transitions ") && t.contains(" to "));
    let create_via_ui = has(&|t| t.starts_with("create round trip persists a new refund request"));
    let details_values = has(&|t| t.starts_with("details shows the persisted values"));

    let missing: Vec<&str> = [
        ("render", render),
        ("persona allow", persona_allow),
        ("persona deny", persona_deny),
        ("click-through", click_through),
        ("validation", validation),
        ("round trip", round_trip),
        ("workflow", workflow),
        ("workflow transition", workflow_transition),
        ("create via ui", create_via_ui),
        ("details values", details_values),
    ]
    .iter()
    .filter(|(_, present)| !present)
    .map(|(name, _)| *name)
    .collect();
    assert!(
        missing.is_empty(),
        "spec report is missing expected categories: {missing:?}\ntitles: {titles:?}"
    );

    // Dialog mapping coverage is structural: the modal view's page must
    // invoke the mapped Dialog component. Only checked while the view
    // exists — T4 removes HelpModal and regenerates.
    let help_modal_path = svelte_dir().join("src/routes/helpmodal/+page.svelte");
    if help_modal_path.exists() {
        let help_modal = fs::read_to_string(&help_modal_path).unwrap_or_default();
        assert!(
            help_modal.contains("<Dialog"),
            "HelpModal page should invoke the mapped Dialog component:\n{help_modal}"
        );
    }

    // Issue #200: sibling xor containers render ONE labeled wrapper inside
    // the landmark view's page and never as standalone routes. Structural
    // here; visibility is asserted by the gate-owned sweep spec.
    assert!(
        !svelte_dir()
            .join("src/routes/shipping/+page.svelte")
            .exists(),
        "nested containers must not be generated as standalone routes"
    );
    assert!(
        !svelte_dir()
            .join("src/routes/payment/+page.svelte")
            .exists(),
        "nested containers must not be generated as standalone routes"
    );

    // Issue #198 workflow UI v2: the details view of the workflow-bound
    // entity must render transition buttons (per valid from → to edge) with
    // the e2e-hook contract. The fixture's first transition is
    // draft → submitted; the fixture workflow config already sets
    // generate_action_endpoints = true so the POST target exists.
    let detail_path = svelte_dir().join("src/routes/refundrequestdetail/+page.svelte");
    if detail_path.exists() {
        let detail = fs::read_to_string(&detail_path).unwrap_or_default();
        assert!(
            detail.contains("data-testid=\"info-transition-submitted\""),
            "workflow-bound details page should render the draft→submitted transition button:\n{detail}"
        );
        assert!(
            detail.contains("data-transition-from=\"draft\"")
                && detail.contains("data-transition-to=\"submitted\""),
            "transition buttons carry data-transition-from/to hooks:\n{detail}"
        );
        assert!(
            detail.contains("/actions/transition"),
            "the transition handler must call the generated workflow_action endpoint:\n{detail}"
        );
    }
    let home_path = svelte_dir().join("src/routes/home/+page.svelte");
    if home_path.exists() {
        let home = fs::read_to_string(&home_path).unwrap_or_default();
        assert!(
            home.contains("data-testid=\"shipping-label\""),
            "Home page should carry the Shipping container label heading:\n{home}"
        );
        assert!(
            home.contains("data-testid=\"payment-label\""),
            "Home page should carry the Payment container label heading:\n{home}"
        );
        assert_eq!(
            home.matches("<Tabs testid=\"tabs\">").count(),
            1,
            "sibling xor containers must share exactly one mapped wrapper:\n{home}"
        );
    }
}

/// T4: removing a view from the .ifml and regenerating into the same root
/// keeps the whole stack green (typecheck + build + specs).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "full-stack gate: run with --ignored (needs node, chromium, postgres)"]
async fn regen_after_view_removal_stays_green() {
    let _guard = gate_lock().lock().await;
    if !have_tool("node") || !have_tool("npx") {
        skip("node/npx not found");
        return;
    }
    let Ok(base) = resolve_base_target().await else {
        skip("no usable postgres (DATABASE_URL + docker fallback failed)");
        return;
    };

    // Full generation first, so the removal path regenerates into a tree
    // that previously contained the view.
    ensure_generated().await.unwrap();

    stage_fixture().unwrap();
    let removed = remove_view_from_ifml(&fixture_home().join("app.ifml"), "HelpModal").unwrap();
    assert!(removed > 100, "expected to remove the HelpModal block");
    run_pipeline().await.unwrap();
    write_extras(&svelte_dir()).unwrap();
    // Wave B fix: the regeneration rewrites the generated Cargo.toml, so the
    // workspace-isolation table appended by ensure_generated() must be
    // re-appended before the server build.
    isolate_generated_workspace().unwrap();

    let project = svelte_project();

    // Wave B note: the full `run` pipeline does not clean stale route dirs of
    // removed views (clean_stale_ifml_routes only runs on the ifml-generate
    // path). Surface it loudly but do not mask the T1-T3 assertions below.
    if svelte_dir().join("src/routes/helpmodal").exists() {
        println!(
            "gate: WARNING removed view's route dir was not cleaned by regeneration \
             (stale-route cleaning is missing from the full pipeline)"
        );
    }

    project.ensure_install().unwrap();
    project.sync_sveltekit().unwrap();

    let Ok((typecheck_ok, output)) = project.run_check() else {
        panic!("failed to launch svelte-check");
    };
    assert!(
        typecheck_ok,
        "svelte-check failed after view removal:\n{}",
        output.lines().take(80).collect::<Vec<_>>().join("\n")
    );

    let build_ok = project.run_build().unwrap();
    assert!(build_ok, "vite build failed after view removal");

    let Ok(()) = playwright::install_chromium(&project) else {
        skip("chromium not available and install failed");
        return;
    };
    let db = GateDb::create(base).await.unwrap();
    db.apply_prelude().await.unwrap();
    db.apply_migrations(&app_dir().join("migrations"))
        .await
        .unwrap();
    let api_key = db.provision_api_key().await.unwrap();

    let result = async {
        let bin = axum_server::build_app(
            &app_dir().join("Cargo.toml"),
            &repo_root(),
            &gate_target_dir(),
            &logs_dir(),
            "ifml-gate-app",
        )?;
        let database_url = db.url();
        let server = axum_server::boot_server(
            &bin,
            &[
                ("DATABASE_URL", database_url.as_str()),
                ("SUPABASE_JWT_SECRET", "ifml-gate-test-secret"),
                ("APP_NAME", "ifml-gate-app"),
            ],
            "ifml-gate-app",
            &logs_dir(),
        )
        .await?;
        let preview_port = free_port();
        let built = project.run_build()?;
        assert!(built, "vite build failed before the spec run");
        let report = playwright::run_specs(
            &project,
            &gate_root().join("playwright-report.json"),
            server.port,
            preview_port,
            &api_key,
        )?;
        assert_eq!(
            report.failed, 0,
            "specs failed after view removal — see target/ifml-gate/playwright-report.json"
        );
        assert_categories(&report.titles);
        Ok::<(), String>(())
    }
    .await;

    db.cleanup().await;
    result.unwrap();
}
