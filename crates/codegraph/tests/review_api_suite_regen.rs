//! Shared review-fixture regeneration (used by `review_api_suite`, the
//! #574 S2 `entity_codegen_gate` and `rules_guard_api_gate`) — ONE recipe so
//! the gates can never drift apart: the full default-profile pipeline over
//! `tests/fixtures/{schemas,classifier.toml,domains.toml}` written into
//! `crates/review/generated-candidate/`, minus the gRPC (protoc) and ops
//! (would overwrite the checked-in manifest/testkit) generators.
//!
//! ## The #694 rule: regenerate from clean, serialized
//!
//! The fixture is a persistent, CHECKED-IN tree that FOUR test paths regenerate
//! in place (this flag-ON recipe's three gates, plus the flag-OFF
//! `grafeo_candidate_inspect_output` / `review_fixture_flag_off_regen`), so two
//! properties are load-bearing and neither is `--test-threads=1`:
//!
//! 1. **From clean** — [`FixtureGate::provision`] sweeps every file this
//!    recipe's PREVIOUS generation produced before regenerating (see
//!    `review_fixture_gate`, and its module docs for why the sweep is per
//!    recipe: the flag-OFF recipe produces `testkit/` + `codegraph-ops.toml`,
//!    which this plan strips and which the live gate needs). The set comes
//!    from the generation report, not a hand-kept allowlist, so committed
//!    extras and gate-owned stubs can never appear in it and are never
//!    deleted. This is the #688 shape: an artifact an older generation emitted
//!    (a renumbered `*_actions.hurl`, a removed view's page class) is
//!    discovered by glob/suffix and read as this run's — the review fixture had
//!    exactly that hole.
//! 2. **Serialized** — [`FixtureGate::acquire`] takes an exclusive
//!    CROSS-PROCESS file lock shared by all four paths. `--test-threads=1` only
//!    serializes one binary, and hides the problem for anyone running a subset.
//!
//! Callers therefore hold a [`FixtureGate`] for the whole test body (not just
//! the regeneration) and pass it to the provision step — the signature makes
//! it impossible to regenerate without the lock.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use codegraph_core::traits::GraphQuerier;

// The cross-process gate + the per-recipe produced-file ledgers (issue #694).
// Shared with the flag-OFF mutators by path so there is exactly one lock and
// one ledger set.
#[allow(dead_code)]
#[path = "review_fixture_gate.rs"]
pub mod review_fixture_gate;

pub use review_fixture_gate::{FixtureGate, Recipe};

pub fn manifest_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

pub fn fixture_dir() -> PathBuf {
    manifest_dir()
        .parent()
        .unwrap()
        .join("review")
        .join("generated-candidate")
}

pub fn ensure_testkit_workspace_member(fixture_dir: &Path) {
    let cargo = fixture_dir.join("Cargo.toml");
    let Ok(text) = std::fs::read_to_string(&cargo) else {
        return;
    };
    if text.contains("\"testkit\"") {
        return;
    }
    let patched = text.replace(
        "members = [\".\", \"cli\", \"migration\"]",
        "members = [\".\", \"cli\", \"migration\", \"testkit\"]",
    );
    if patched != text {
        let _ = std::fs::write(&cargo, patched);
    }
}

pub fn ensure_testkit_path_dep(fixture_dir: &Path) {
    let cargo = fixture_dir.join("testkit").join("Cargo.toml");
    let Ok(text) = std::fs::read_to_string(&cargo) else {
        return;
    };
    let patched = text
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("codegraph-ops") {
                "codegraph-ops = { path = \"../../../codegraph-ops\" }".to_string()
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if patched != text {
        let _ = std::fs::write(&cargo, patched);
    }
}

/// Regenerate the fixture app in place from the current templates.
///
/// - `output_dir` is the checked-in fixture dir (the fixture tracks the
///   branch, like `grafeo_candidate_inspect_output`);
/// - the domain-types crate is generated INSIDE the app output so the
///   scaffolded `domain-types = { path = "domain-types" }` dependency
///   resolves;
/// - the gRPC generators (need protoc) and the `ops` generator (would
///   overwrite the checked-in `codegraph-ops.toml` + `testkit/`) are stripped
///   from the plan.
///
/// Returns every file the run wrote, output-dir relative (the #694 ledger's
/// content). Callers go through [`FixtureGate::provision`], which needs the
/// [`FixtureGate`] and starts from clean.
async fn regenerate_fixture() -> Result<Vec<PathBuf>, String> {
    use codegraph::generate::template_engine::create_tera;
    use codegraph::generate::{
        DtoKeyCasing, GeneratorOpts, ProjectConfig, compute_generation_order,
        run_generators_with_opts,
    };
    use codegraph::profile::{BuildPlan, CapabilityRegistry, load_and_resolve_profile};
    use codegraph_classifier::config::parse_classifier_config;
    use codegraph_config::UiOverrideConfig;
    use codegraph_grafeo::GrafeoEngine;

    let mdir = manifest_dir();
    let workspace_root = mdir.parent().unwrap().parent().unwrap();
    let output_dir = fixture_dir();

    let config =
        codegraph_config::config::parse_domain_config(&mdir.join("tests/fixtures/domains.toml"))
            .map_err(|e| format!("parse domains.toml: {e}"))?;
    let classifier = parse_classifier_config(&mdir.join("tests/fixtures/classifier.toml"))
        .map_err(|e| format!("parse classifier.toml: {e}"))?;
    let entity_names: HashSet<String> = config
        .domains
        .values()
        .flat_map(|d| d.entities.iter().cloned())
        .collect();

    let engine = GrafeoEngine::in_memory().map_err(|e| format!("grafeo: {e}"))?;
    // Pass 1b (the driver's ordering): the rosetta eligibility-rule model
    // (issue #338) bridges before the JSON pass — the GrantApplication
    // entity + its `PositiveGrant` rule give the api suite a modeled guard
    // to exercise over real HTTP.
    let rosetta_model = mdir.join("tests/fixtures/rosetta/grant.rosetta");
    let rosetta_outcome = codegraph::ingest::rosetta_ingest::ingest_rosetta_files(
        &engine,
        &engine,
        &[rosetta_model],
        &config,
        &config.defaults.type_suffix,
    )
    .await
    .map_err(|e| format!("ingest fixture rosetta model: {e}"))?;
    if rosetta_outcome.stats.rules_ingested == 0 {
        return Err("the rosetta fixture model bridged zero rules — the eligibility guard cannot be exercised".into());
    }
    codegraph::ingest::async_ingest::ingest_schemas(
        &engine,
        Path::new("tests/fixtures/schemas"),
        &classifier,
        &entity_names,
        &UiOverrideConfig::default(),
        &config.defaults.type_suffix,
    )
    .await
    .map_err(|e| format!("ingest fixture schemas: {e}"))?;

    // Auto-classify (the driver's pass): rosetta-bridged types are
    // auto-scored — GrantApplicationType must land as an ENTITY or the
    // guarded handler plane never generates.
    let classifier_types: HashSet<String> = classifier
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
        .map_err(|e| format!("classification data: {e}"))?;
    let auto_classifier =
        codegraph::classify::AutoClassifier::new(classifier_types, classifier.naming_rules.clone());
    let mut classified_entities = entity_names.clone();
    let mut sorted_domain_names: Vec<&String> = config.domains.keys().collect();
    sorted_domain_names.sort();
    for domain_name in &sorted_domain_names {
        let domain_entry = &config.domains[domain_name.as_str()];
        let domain_schemas: Vec<_> = all_data
            .iter()
            .filter(|d| d.domain.as_deref() == Some(domain_name.as_str()))
            .cloned()
            .collect();
        let result = auto_classifier.classify_domain(domain_name, domain_entry, &domain_schemas);
        for score in &result.entities {
            classified_entities.insert(score.title.clone());
        }
    }
    codegraph::ingest::async_ingest::reclassify_with_entities(
        &engine,
        &engine,
        &classified_entities,
    )
    .await
    .map_err(|e| format!("reclassify with entities: {e}"))?;
    if !classified_entities.contains("GrantApplicationType") {
        return Err(
            "the rosetta fixture entity did not auto-classify as an entity — the guarded \
             handler plane would never generate"
                .into(),
        );
    }

    // The generated app must compile without touching the network: path deps
    // into this workspace, domain-types inside the app output.
    let type_contracts_path = workspace_root.join("crates/codegraph-type-contracts");
    let workflow_path = workspace_root.join("crates/codegraph-workflow");

    let registry = CapabilityRegistry::new();
    let mut resolved =
        load_and_resolve_profile(&workspace_root.join("profiles.toml"), "default", None)
            .map_err(|e| format!("resolve profile: {e}"))?;
    for section in resolved.sections.values_mut() {
        section
            .generators
            .retain(|g| !g.starts_with("grpc_") && g != "ops");
    }
    // The eligibility-rule guard plane (issue #338): the fixture plan
    // carries the `rules` generator (requires the `rosetta_backend`
    // feature) so the emitted app ships `src/domain/recruiting/rules.rs`
    // and the handler guards that call into it.
    resolved
        .features
        .insert("rosetta_backend".into(), toml::Value::Boolean(true));
    if let Some(api) = resolved.sections.get_mut("api")
        && !api.generators.iter().any(|g| g == "rules")
    {
        api.generators.push("rules".into());
    }
    let plan =
        BuildPlan::from_profile(&resolved, &registry).map_err(|e| format!("build plan: {e}"))?;

    let project_config = ProjectConfig {
        identity: codegraph::generate::IdentityConfig {
            app_name: "app".into(),
            domain_types_crate: "domain_types".into(),
            generator_name: "codegraph-review-gate".into(),
            ..Default::default()
        },
        paths: codegraph::generate::PathsConfig {
            type_contracts_base: type_contracts_path.to_string_lossy().to_string(),
            codegraph_workflow_base: workflow_path.to_string_lossy().to_string(),
            domain_types_base: "domain-types".into(),
            ..Default::default()
        },
        cargo: codegraph::generate::CargoConfig {
            extra_dependencies: format!(
                "codegraph-workflow = {{ path = \"{}\" }}\n\
                 codegraph-type-contracts = {{ path = \"{}\" }}",
                workflow_path.display(),
                type_contracts_path.display()
            ),
            ..Default::default()
        },
        codegen: codegraph::generate::CodegenConfig {
            // Thread the plan's codegen flags (the driver does the same):
            // without this the fixture regenerates with every codegen flag
            // defaulted OFF and the gate validates a plane the shipped
            // profiles never produce.
            dto_key_casing: DtoKeyCasing::from_config(&plan.dto_key_casing),
            namespace_layout: plan.namespace_layout,
            expr_ir: plan.expr_ir,
            ifml_e2e_auth: plan.ifml_e2e_auth,
            ifml_remote_functions: plan.ifml_remote_functions,
            entity_remote_functions: plan.entity_remote_functions,
            entity_form: plan.entity_form,
            ifml_tanstack_forms: plan.ifml_tanstack_forms,
            ui_live_queries: plan.ui_live_queries,
            testid_module: plan.testid_module,
            structural_gates: plan.structural_gates,
            event_envelope: plan.event_envelope.as_str().to_string(),
            event_idempotency: plan.event_idempotency,
            event_archive: plan.event_archive,
            events_producer: plan.events_producer,
            types_import_prefix: config.defaults.types_import_prefix.clone(),
        },
        ..Default::default()
    };

    let domain_types_dir = output_dir.join("domain-types");
    let hooks_tmp = tempfile::tempdir().map_err(|e| format!("hooks tempdir: {e}"))?;

    // The generation order is needed twice: by the pipeline itself and to
    // assert the graph actually produced entities (an empty graph would
    // "regenerate" nothing and let a stale fixture pass).
    let order = compute_generation_order(&engine, &config)
        .await
        .map_err(|e| format!("generation order: {e}"))?;
    if order.is_empty() {
        return Err("generation order is empty — fixture schemas failed to ingest".into());
    }

    let tera = create_tera(Path::new("unused")).map_err(|e| format!("tera: {e}"))?;
    let report = run_generators_with_opts(GeneratorOpts {
        integration_manifests: None,
        db: &engine,
        config: &config,
        output_dir: &output_dir,
        tera: &tera,
        ui_overrides: &UiOverrideConfig::default(),
        ui_domains: &codegraph_config::UiDomainConfig::default(),
        schema_base_dir: Path::new(""),
        seed_config: None,
        domain_types_base: Some(&domain_types_dir),
        hooks_base: Some(hooks_tmp.path()),
        ext_points: None,
        build_plan: Some(&plan),
        profile_name: "default",
        ifml_frameworks: vec![],
        ifml_components: None,
        ux_rules: None,
        project_config: Some(&project_config),
        emdash_plugins: None,
        domain_config_dir: None,
    })
    .await
    .map_err(|e| format!("regenerate fixture: {e}"))?;

    if report.has_errors() {
        let detail = report
            .errors
            .iter()
            .map(|e| format!("{}/{}: {}", e.entity, e.generator, e.source))
            .take(10)
            .collect::<Vec<_>>()
            .join("\n");
        return Err(format!("fixture regeneration reported errors:\n{detail}"));
    }

    ensure_testkit_workspace_member(&output_dir);
    ensure_testkit_path_dep(&output_dir);

    // The ledger's content: exactly the files this run wrote (the report
    // mirrors every `write_output` call in the pipeline). Absolute paths —
    // `review_fixture_gate::ledger_text` relativizes them against the fixture
    // and drops anything redirected outside it (the hooks tempdir).
    Ok(report.files.iter().map(|file| file.path.clone()).collect())
}

/// Provision the review fixture (issue #694): from clean, under the gate.
///
/// 1. sweep every file a previous generation of EITHER recipe produced (see
///    `review_fixture_gate::sweep_produced`) so no artifact of an older
///    generation — including flag-OFF residue this recipe never emits — can be
///    discovered as this run's;
/// 2. regenerate with [`regenerate_fixture`];
/// 3. record this recipe's new produced set.
///
/// It is a METHOD on the guard, not a free function, because the caller must
/// hold the gate for its whole body (the gates BUILD in this tree after
/// provisioning), and two `flock`s on the same file from one process would
/// deadlock — the signature makes it impossible to provision without it.
impl FixtureGate {
    pub async fn provision(&self) -> Result<(), String> {
        let output_dir = fixture_dir();
        // Cross-recipe stale contracts (#694/#764): the committed tree is the
        // FLAG-OFF recipe's, whose hurl numbering excludes the
        // rosetta/auto-classify entities this flag-ON plan adds. Those files
        // are not in the flag-ON ledger (sweeps are per recipe — see
        // `review_fixture_gate`), so the sweep cannot remove them, and the api
        // suite's hurl stage discovers every `*.hurl` in the directory. The
        // flag-ON plan re-emits the whole dir, so make it from-clean first.
        let hurl_dir = output_dir.join("hurl");
        if hurl_dir.is_dir() {
            std::fs::remove_dir_all(&hurl_dir).map_err(|e| {
                format!(
                    "sweep the stale generated hurl dir {}: {e}",
                    hurl_dir.display()
                )
            })?;
            eprintln!(
                "provision: removed the stale generated hurl dir {} — a previous recipe's \
                 contracts would otherwise run as this run's",
                hurl_dir.display()
            );
        }
        let swept = review_fixture_gate::sweep_produced(&output_dir, Recipe::FlagOn);
        if swept > 0 {
            eprintln!(
                "provision: swept {swept} file(s) written by the previous flag-ON generation \
                 of {}",
                output_dir.display()
            );
        }
        let produced = regenerate_fixture().await?;
        review_fixture_gate::record_produced(&output_dir, Recipe::FlagOn, &produced)
    }
}
