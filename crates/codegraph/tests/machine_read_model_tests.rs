//! The machine read-model contract (issue #817 — Lane 3 of epic #812).
//!
//! Node-free pins over the REAL pipeline (the `ifml_codegen_gate`
//! fixture, in-process, no node/npm/DB): the fixture's `claims` module
//! binds a TypeScript-authored machine (the Lane-1 spelling) to `Claim`,
//! whose domains.toml workflow config supplies the transition POST
//! endpoint. Every surface the contract promises is pinned here:
//!
//! 1. **Kernel derivation** — state X → exactly the declared out-edges;
//!    an unknown current state is VISIBLE (`state_known = false`) with an
//!    EMPTY edge set and the drift hash present (the UI can never offer a
//!    transition the backend would 409).
//! 2. **Route/handler emission, presence-gated** — the machine-bearing
//!    entity emits `GET /machine` (router + handler + OpenAPI); the
//!    machine-less twin (the refunds entity's inline DSL machine carries
//!    no kernel IR) emits nothing.
//! 3. **The merged `/workflow` payload** — dual entities merge the same
//!    read model (`machine` object) via the sibling module's ONE
//!    derivation.
//! 4. **IFML consumption** — buttons render from `available_transitions`
//!    (a runtime each-loop), the badge from `current_state`, the remote
//!    module gains `get{Pascal}Machine` on the read-model route.
//! 5. **POM** — `machineState()` / `machineTransitions()` /
//!    `machineTransition(event)` page-class methods; the spec family
//!    drives them with zero raw testids.
//! 6. **No browser runtime** — no `xstate`/`restingstate` package or
//!    import enters the frontend bundle: the UI renders a derived read
//!    model, it never runs a machine.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use codegraph_core::traits::GraphQuerier;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ifml_gate")
}

/// One shared generation: the fixture pipeline into `target/machine-read-model/`,
/// files indexed by repo-relative path. The pipeline is deterministic, so
/// every pin reads the same tree.
fn generated() -> &'static HashMap<String, String> {
    static TREE: OnceLock<HashMap<String, String>> = OnceLock::new();
    TREE.get_or_init(|| {
        let tree = tokio::runtime::Runtime::new()
            .expect("runtime")
            .block_on(generate_fixture_tree());
        match tree {
            Ok(tree) => tree,
            Err(e) => panic!("fixture pipeline failed: {e}"),
        }
    })
}

fn read(rel: &str) -> &'static str {
    generated()
        .get(rel)
        .unwrap_or_else(|| panic!("expected generated file `{rel}`; have:\n{}", keys()))
}

fn keys() -> String {
    let mut names: Vec<&String> = generated().keys().collect();
    names.sort();
    names
        .iter()
        .map(|k| k.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn must_not_have(rel: &str) {
    assert!(
        !generated().contains_key(rel),
        "machine-less surface `{rel}` must not be emitted (presence gate)"
    );
}

/// The fixture pipeline (the `ifml_codegen_gate::run_pipeline` shape,
/// node-free): schemas + IFML front door + `.ddd` ingest + classify +
/// `run_generators_with_opts` over the gate profile.
async fn generate_fixture_tree() -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    use codegraph_config::UiOverrideConfig;

    let home = fixture_dir();
    let output = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/machine-read-model/app");
    let _ = std::fs::remove_dir_all(&output);
    std::fs::create_dir_all(&output)?;

    let config = codegraph_config::config::parse_domain_config(&home.join("domains.toml"))?;
    let classifier =
        codegraph_classifier::config::parse_classifier_config(&home.join("classifier.toml"))?;
    let registry = codegraph::profile::CapabilityRegistry::new();
    let resolved =
        codegraph::profile::load_and_resolve_profile(&home.join("profiles.toml"), "gate", None)?;
    let plan = codegraph::profile::BuildPlan::from_profile(&resolved, &registry)?;

    let engine = codegraph_grafeo::GrafeoEngine::in_memory()?;

    let empty_entities = std::collections::HashSet::new();
    let ui_overrides = UiOverrideConfig::default();
    codegraph::ingest::async_ingest::ingest_schemas(
        &engine,
        &home.join("schemas"),
        &classifier,
        &empty_entities,
        &ui_overrides,
        &config.defaults.type_suffix,
    )
    .await?;

    let ifml_path = home.join("app.ifml");
    let compiled = codegraph::ifml_compile::compile_and_check_ifml(&ifml_path)?;
    codegraph::ingest::ifml_ingest::ingest_ifml_model(&engine, &compiled.compilation.model).await?;
    codegraph::ingest::ifml_ingest::ingest_ifml_manifest(
        &engine,
        &compiled.manifest,
        &[ifml_path.display().to_string()],
    )
    .await?;
    codegraph::ifml_actor_import::ingest_actor_imports(
        &engine,
        &engine,
        &compiled.compilation.model,
        &ifml_path,
    )
    .await?;

    codegraph::ingest::ddd_ingest::ingest_ddd_files(
        &engine,
        &engine,
        &[home.join("refunds.ddd")],
        &config,
    )
    .await?;

    codegraph::ingest::api_ingest::ingest_api_model(&engine, &config).await?;

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
    let all_data = engine.get_classification_data().await?;
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
        .await?;

    let templates = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = codegraph::generate::template_engine::create_tera(&templates)?;

    let workspace = repo_root();
    let type_contracts = workspace.join("crates/codegraph-type-contracts");
    let workflow = workspace.join("crates/codegraph-workflow");
    let domain_types_dir = output.join("domain-types");

    let project_config = codegraph::generate::ProjectConfig {
        identity: codegraph::generate::IdentityConfig {
            app_name: "ifml-gate-app".into(),
            lib_name: "cosmos".into(),
            domain_types_crate: "gate_domain_types".into(),
            api_title: "IFML Gate API".into(),
            generator_name: "codegraph-gate".into(),
            api_version: config.defaults.api_version.clone(),
            ..Default::default()
        },
        paths: codegraph::generate::PathsConfig {
            domain_types_base: "domain-types".into(),
            type_contracts_base: type_contracts.to_string_lossy().to_string(),
            codegraph_workflow_base: workflow.to_string_lossy().to_string(),
            ..Default::default()
        },
        database: codegraph::generate::DatabaseConfig {
            database_target: codegraph::generate::db::dialect::DatabaseTarget::Postgres,
            persistence_provider: codegraph::profile::PersistenceProvider::SeaOrm,
        },
        deployment: codegraph::generate::DeploymentConfig {
            deployment_topology: codegraph::profile::DeploymentTopology::Monolith,
        },
        codegen: codegraph::generate::CodegenConfig {
            types_import_prefix: config.defaults.types_import_prefix.clone(),
            ifml_e2e_auth: plan.ifml_e2e_auth,
            ifml_remote_functions: plan.ifml_remote_functions,
            ifml_tanstack_forms: plan.ifml_tanstack_forms,
            entity_form: plan.entity_form,
            ui_live_queries: plan.ui_live_queries,
            ..Default::default()
        },
        cargo: codegraph::generate::CargoConfig {
            extra_dependencies: format!(
                "codegraph-workflow = {{ path = \"{}\" }}\n\
                 codegraph-type-contracts = {{ path = \"{}\" }}",
                workflow.display(),
                type_contracts.display(),
            ),
            ..Default::default()
        },
        ..Default::default()
    };

    let fixture_mappings_path = home.join("ifml-components.toml");
    let pack = if fixture_mappings_path.exists() {
        codegraph_config::IfmlComponentMappings::load(&fixture_mappings_path)?
    } else {
        codegraph_config::IfmlComponentMappings::default()
    };
    let hooks_tmp = tempfile::tempdir()?;

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
            integration_manifests: None,
            build_plan: Some(&plan),
            profile_name: "default",
            ifml_frameworks: vec!["svelte".to_string()],
            ifml_components: Some(&pack),
            ux_rules: None,
            project_config: Some(&project_config),
            emdash_plugins: None,
            domain_config_dir: Some(&home),
        })
        .await?;

    assert!(
        !report.has_errors(),
        "generation reported errors: {:#?}",
        report
            .errors
            .iter()
            .map(|e| format!("{}/{}: {}", e.entity, e.generator, e.source))
            .collect::<Vec<_>>()
    );

    let mut tree = HashMap::new();
    collect_tree(&output, &output, &mut tree);
    Ok(tree)
}

fn collect_tree(root: &Path, dir: &Path, out: &mut HashMap<String, String>) {
    for entry in std::fs::read_dir(dir).expect("read_dir") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.is_dir() {
            collect_tree(root, &path, out);
        } else {
            let rel = path
                .strip_prefix(root)
                .expect("relative")
                .to_string_lossy()
                .to_string();
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            out.insert(rel, content);
        }
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

// ── 1. Kernel derivation (cross-plane KATs over the fixture's machine) ────

/// The fixture's machine resolves and derives: `draft` → exactly the one
/// declared out-edge (SUBMIT, draft → submitted), hash present.
#[tokio::test]
async fn the_fixture_machine_derives_the_read_model_from_the_kernel() {
    let engine = fixture_engine().await;
    let surface = codegraph_generate::machines::machine_surface_for_entity(
        &engine,
        "ClaimType",
        Some("status"),
    )
    .await
    .expect("Claim must be machine-bearing");
    assert_eq!(surface.machine_id, "claimstatus");
    assert_eq!(surface.status_field, "status");
    assert_eq!(surface.initial_state, "draft");
    assert_eq!(surface.terminal_states, vec!["submitted"]);

    let ir: serde_json::Value = serde_json::from_str(&surface.ir_json).expect("canonical IR");
    let runtime = codegraph_workflow::MachineRuntime::from_json(&ir).expect("valid machine");

    // State X → exactly the declared out-edges, in declaration order.
    let model = runtime.read_model("draft");
    assert_eq!(model.machine_id, "claimstatus");
    assert_eq!(model.current_state, "draft");
    assert!(model.state_known);
    assert!(!model.is_terminal);
    assert_eq!(model.available_transitions.len(), 1, "{model:?}");
    let edge = &model.available_transitions[0];
    assert_eq!(edge.event, "SUBMIT");
    assert_eq!(edge.from, "draft");
    assert_eq!(edge.to, "submitted");
    assert_eq!(edge.guard, None);
    assert_eq!(model.machine_hash, surface.machine_hash);
    assert_eq!(model.machine_hash, runtime.content_hash());

    // The terminal state: no out-edges, is_terminal true.
    let done = runtime.read_model("submitted");
    assert!(done.is_terminal);
    assert!(done.available_transitions.is_empty());

    // An unknown column value: visible, empty, hash present — the UI can
    // never offer a transition the backend would 409.
    let drifted = runtime.read_model("Bogus");
    assert!(!drifted.state_known);
    assert!(!drifted.is_terminal);
    assert!(drifted.available_transitions.is_empty());
    assert_eq!(drifted.machine_hash, runtime.content_hash());
}

async fn fixture_engine() -> impl GraphQuerier {
    let home = fixture_dir();
    let config = codegraph_config::config::parse_domain_config(&home.join("domains.toml"))
        .expect("domains.toml");
    let classifier =
        codegraph_classifier::config::parse_classifier_config(&home.join("classifier.toml"))
            .expect("classifier.toml");
    let engine = codegraph_grafeo::GrafeoEngine::in_memory().expect("engine");
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
    .expect("schemas");
    let ifml_path = home.join("app.ifml");
    let compiled = codegraph::ifml_compile::compile_and_check_ifml(&ifml_path).expect("ifml");
    codegraph::ingest::ifml_ingest::ingest_ifml_model(&engine, &compiled.compilation.model)
        .await
        .expect("ifml model");
    codegraph::ingest::ddd_ingest::ingest_ddd_files(
        &engine,
        &engine,
        &[home.join("refunds.ddd")],
        &config,
    )
    .await
    .expect("ddd");
    engine
}

// ── 2. Route/handler/OpenAPI emission — presence-gated twins ──────────────

/// The machine-bearing entity emits the `/machine` route, the handler
/// module, and the OpenAPI registration.
#[test]
fn the_machine_bearing_entity_emits_the_machine_read_model_route() {
    let router = read("src/api/claims/router.rs");
    assert!(
        router.contains("claim_machine::get_machine"),
        "router must mount the machine route:\n{}",
        router
            .lines()
            .filter(|l| l.contains("machine"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        router.contains("/machine"),
        "router must carry the /machine path segment"
    );

    let handler = read("src/api/claims/claim_machine.rs");
    assert!(handler.contains("pub async fn get_machine"), "{handler}");
    // THE one derivation: entity column + kernel registry, hash included.
    assert!(
        handler.contains("machine_read_model"),
        "the handler must carry the read-model derivation"
    );
    assert!(
        handler.contains("machine_hash"),
        "the payload must carry the machine drift hash"
    );
    assert!(
        handler.contains("MACHINE_ID: &str = \"claimstatus\""),
        "the handler must pin its registry id"
    );
    assert!(
        handler.contains("state_known"),
        "an unknown column value must be visible in the payload"
    );

    let openapi = read("src/api/openapi/claims.rs");
    assert!(
        openapi.contains("claim_machine::get_machine"),
        "OpenAPI must register the machine route"
    );
    assert!(
        openapi.contains("MachineReadModelResponse"),
        "OpenAPI must register the read-model schema"
    );
}

/// The machine-less twin: the refunds entity's INLINE DSL machine carries
/// no kernel IR, so no machine module, no route, no OpenAPI entry.
#[test]
fn the_inline_machine_stays_off_the_workflow_plane() {
    // Issue #818: the refunds escalation machine is INLINE (a DSL mirror
    // with no artifact IR) — but it is NOT the machine the entity's
    // workflow config binds (the config binds `status`), so the workflow-
    // plane read-model surface stays ABSENT: a foreign field's machine
    // would promise transitions the backend 409s. The kernel still covers
    // it (the registry embed + the kernel-probed action routes); only the
    // read-model surface is scoped. Presence gating for the truly
    // machine-less shape is pinned by `machine_kernel_tests` (zero files).
    must_not_have("src/api/refunds/refund_request_machine.rs");
    let router = read("src/api/refunds/router.rs");
    assert!(
        !router.contains("/machine"),
        "a foreign-field machine must not mount the read-model route:\n{}",
        router
            .lines()
            .filter(|l| l.contains("machine"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let openapi = read("src/api/openapi/refunds.rs");
    assert!(
        !openapi.contains("machine"),
        "a foreign-field machine must not register read-model surface:\n{}",
        openapi
            .lines()
            .filter(|l| l.contains("machine"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

// ── 3. The merged /workflow payload (one shape, two planes) ───────────────

/// The dual entity (machine + workflow config) merges the read model into
/// `/workflow` through the sibling module's ONE derivation, and the
/// machine-only surface does not fork a second payload.
#[test]
fn the_merged_workflow_payload_carries_the_machine_object() {
    let workflow = read("src/api/claims/claim_workflow.rs");
    assert!(
        workflow.contains("claim_machine::machine_read_model"),
        "the /workflow read must reuse the sibling module's derivation:\n{}",
        workflow
            .lines()
            .filter(|l| l.contains("machine"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        workflow.contains("\"machine\""),
        "the payload must nest the machine read model"
    );
    // The wire contract of the POST endpoints is unchanged.
    assert!(
        workflow.contains("target_state: body.target_state"),
        "the transition POST body contract must not move"
    );
    assert!(
        workflow.contains("WorkflowError::InvalidTransition"),
        "the 409 mapping must stay"
    );
}

// ── 4. IFML consumption: buttons from available_transitions ───────────────

/// The machine-bearing component's buttons render from the read model (a
/// runtime each-loop over the kernel-derived edges); the machine-less
/// twin keeps the per-declared-edge static loop.
#[test]
fn the_ifml_page_renders_transition_buttons_from_the_read_model() {
    let page = read("svelte/src/routes/claimdetail/+page.svelte");
    assert!(
        page.contains("machine?.available_transitions"),
        "the button set must derive from the read model's available_transitions:\n{}",
        page.lines()
            .filter(|l| l.contains("machine"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        page.contains("machineTransitionKey(t.to)"),
        "the buttons render per runtime edge (testid from the edge target)"
    );
    assert!(
        page.contains("#each"),
        "the buttons render via a runtime each-loop"
    );
    assert!(
        page.contains("available_transitions"),
        "the button set reads available_transitions"
    );
    // No static per-declared-edge button for the machine-bearing component.
    assert!(
        !page.contains("claim_info-transition-draft"),
        "buttons must not render per declared edge (read-model only)"
    );

    // The badge reads the read model's current state.
    assert!(
        page.contains("claim_info-state"),
        "the badge testid contract holds"
    );

    // The foreign-field twin (issue #818): the refund detail page keeps
    // the workflow's own static transition buttons — the escalation
    // machine is NOT the workflow-field machine, so its edges never
    // render through the workflow's buttons (they would 409).
    let refund_page = read("svelte/src/routes/refundrequestdetail/+page.svelte");
    assert!(
        refund_page.contains("info-transition-submitted"),
        "the foreign-field twin keeps the declared-edge buttons (byte identity)"
    );
    assert!(
        !refund_page.contains("machine?.available_transitions"),
        "the foreign-field twin gains no machine-driven buttons"
    );
}

/// The remote plane on the DUAL fixture entity: the existing workflow
/// live poll is EXTENDED (the merged /workflow payload now carries the
/// machine object the buttons read) — never duplicated. Machine-ONLY
/// entities (no /workflow route) gain `get{Pascal}Machine` +
/// `get{Pascal}MachineLive` on `/machine`; the emitter shape is pinned by
/// the in-crate unit test (`route_generator::tests::
/// machine_only_module_gains_the_machine_query_and_live_stream`).
#[test]
fn the_remote_module_extends_the_workflow_live_poll_with_the_machine_fields() {
    let module = read("svelte/src/routes/claimdetail/data.remote.ts");
    // ONE live stream, polling the merged read-model route.
    let live_exports: Vec<&str> = module
        .lines()
        .filter(|l| l.contains("query.live"))
        .collect();
    assert_eq!(
        live_exports.len(),
        1,
        "one live stream per claimant, never duplicated: {live_exports:?}"
    );
    assert!(
        module.contains("getClaimInfoWorkflowLive"),
        "the dual entity keeps the workflow live poll:\n{module}"
    );
    assert!(
        module.contains("/workflow"),
        "the live poll fetches the merged /workflow payload (machine rides it)"
    );
    assert!(
        !module.contains("/machine"),
        "a dual entity must NOT gain a parallel machine query (no duplicated payload)"
    );

    // The machine-only emitter shape (in-crate): get{Pascal}Machine +
    // get{Pascal}MachineLive both on /machine, the load merge normalizing
    // into the same `workflow_state` field.
    let machine_only = codegraph_generate::ifml::route_generator::tests_support_machine_module();
    assert!(
        machine_only.contains("getClaimInfoMachine"),
        "machine-only modules gain the machine query:\n{machine_only}"
    );
    assert!(
        machine_only.contains("getClaimInfoMachineLive"),
        "machine-only modules gain the machine live stream"
    );
    assert!(
        machine_only.contains("/machine"),
        "the machine query fetches the read-model route"
    );
    assert!(
        machine_only.contains("workflow_state") || machine_only.contains("{ ...machine, machine }"),
        "the read model normalizes into the workflow_state shape the page reads"
    );
    let live_count = machine_only
        .lines()
        .filter(|l| l.contains("query.live"))
        .count();
    assert_eq!(live_count, 1, "exactly one live stream");
}

// ── 5. The POM: page-class methods, zero raw testids ──────────────────────

#[test]
fn the_pom_gains_machine_methods_for_the_machine_bearing_component() {
    let pom = read("svelte/tests/pages/claim-detail-page.ts");
    // Component-scoped names (the `transition{Pascal}To` convention).
    for method in [
        "machineClaimInfoState",
        "machineClaimInfoTransitions",
        "machineClaimInfoTransition(event",
    ] {
        assert!(
            pom.contains(method),
            "the page class must carry {method}:\n{}",
            pom.lines()
                .filter(|l| l.contains("machine"))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
    // The selectors live in the page class (zero raw testids in specs).
    assert!(
        pom.contains("\"[data-testid^='claim_info-transition-'][data-transition-event]\""),
        "the transition-set locator is scoped to the component's buttons:\n{pom}"
    );
    // The machine-less twin carries none.
    let refund_pom = read("svelte/tests/pages/refund-request-detail-page.ts");
    assert!(
        !refund_pom.contains("machineTransition("),
        "the machine-less page class gains no machine methods"
    );
}

#[test]
fn the_machine_spec_family_drives_the_pom_with_zero_raw_testids() {
    let spec = read("svelte/tests/ifml/claim-detail.machine.spec.ts");
    assert!(
        spec.contains("Page"),
        "the spec drives the view's page class"
    );
    assert!(
        spec.contains("machineClaimInfoTransitions()"),
        "the spec asserts the rendered button set through the POM"
    );
    assert!(
        spec.contains("machineClaimInfoTransition(edge.event)"),
        "the spec clicks the rendered edge through the POM"
    );
    assert!(
        spec.contains("409"),
        "the spec pins the not-offered transition's 409"
    );
    // Zero raw testids in the spec body (the #315 discipline): the page
    // class owns every selector.
    assert!(
        !spec.contains("data-testid"),
        "spec bodies must not carry raw testids:\n{spec}"
    );
}

// ── 6. No browser runtime: the UI renders a read model, never a machine ──

#[test]
fn no_machine_runtime_package_enters_the_frontend() {
    for (rel, content) in generated().iter() {
        if rel.ends_with("package.json") {
            assert!(
                !content.contains("\"xstate\"") && !content.contains("restingstate"),
                "{rel} must not depend on a machine runtime package:\n{content}"
            );
        }
        if rel.starts_with("svelte/src") || rel.starts_with("svelte/tests") {
            assert!(
                !content.contains("from 'xstate'") && !content.contains("from \"xstate\""),
                "{rel} must not import a machine runtime"
            );
            assert!(
                !content.to_lowercase().contains("restingstate"),
                "{rel} must not reference a machine runtime package"
            );
        }
    }
}

/// The kernel registry lives server-side (domain-types) — the machine IR
/// never crosses to the frontend tree.
#[test]
fn the_kernel_registry_stays_in_the_domain_types_crate() {
    let registry = read("domain-types/src/machines/mod.rs");
    assert!(
        registry.contains("claimstatus"),
        "the registry embeds the fixture machine"
    );
    let ir = read("domain-types/src/machines/claims/claimstatus.ir.json");
    assert!(
        ir.contains("\"SUBMIT\""),
        "the embed is the canonical IR: {ir}"
    );
}
