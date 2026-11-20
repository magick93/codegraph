//! Shared review-fixture regeneration (used by `review_api_suite`, the
//! #574 S2 `entity_codegen_gate` and `rules_guard_api_gate`) — ONE recipe so
//! the gates can never drift apart: the full default-profile pipeline over
//! `tests/fixtures/{schemas,classifier.toml,domains.toml}` written into
//! `crates/review/generated-candidate/`, minus the gRPC (protoc) and ops
//! (would overwrite the checked-in manifest/testkit) generators.
//!
//! ## The #694 rule: regenerate from clean, serialized
//!
//! The fixture is a persistent, CHECKED-IN tree that three gate binaries
//! regenerate in place, so two properties are load-bearing and neither is
//! `--test-threads=1`:
//!
//! 1. **From clean** — [`FixtureGate::provision`] deletes every file the
//!    PREVIOUS provision produced before regenerating. The previous set is
//!    recorded from the generation report (the exact files the pipeline
//!    wrote), not guessed from a hand-kept allowlist, so committed extras
//!    (`testkit/`, `codegraph-ops.toml`, `ui/forms/`, the retired-era trees)
//!    and gate-owned stubs can never appear in it and are never deleted. This
//!    is the #688 shape: an artifact an older generation emitted (a
//!    renumbered `*_actions.hurl`, a removed view's page class) is discovered
//!    by glob/suffix and read as this run's — the review fixture had exactly
//!    that hole.
//! 2. **Serialized** — [`FixtureGate::acquire`] takes an exclusive
//!    CROSS-PROCESS file lock. `--test-threads=1` only serializes one binary,
//!    and hides the problem for anyone running a subset; the lock covers the
//!    three binaries and any thread count.
//!
//! Callers therefore hold a [`FixtureGate`] for the whole test body (not just
//! the regeneration) and pass it to the provision step — the signature makes
//! it impossible to regenerate without the lock.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use codegraph_core::traits::GraphQuerier;

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

/// Run state for the review-fixture gates (issue #694): the cross-process
/// lock and the produced-file ledger. Lives in the workspace `target/` — never
/// inside the committed fixture (machine state must not reach the tracked
/// bytes) and never in `crates/codegraph/target/` (issue #683a: that directory
/// is not the workspace target dir).
fn gate_state_dir() -> PathBuf {
    manifest_dir()
        .parent()
        .and_then(Path::parent)
        .expect("codegraph crate lives at <repo>/crates/codegraph")
        .join("target")
        .join("review-fixture-gate")
}

fn gate_lock_path() -> PathBuf {
    gate_state_dir().join("gate.lock")
}

/// One absolute-or-relative path per line: every file the LAST provision of
/// this recipe wrote. [`FixtureGate::provision`] deletes those files before
/// regenerating (the #688 from-clean rule) and rewrites the ledger afterwards.
fn produced_ledger_path() -> PathBuf {
    gate_state_dir().join("produced.txt")
}

/// The exclusive, cross-process gate over the review fixture.
///
/// Held for a test's whole body: the three gates regenerate AND build in the
/// same tree, so serializing only the regeneration would leave the builds
/// interleaving a tree another test is rewriting. Dropping it releases the
/// lock (the OS also releases it if the process dies).
pub struct FixtureGate {
    file: std::fs::File,
}

impl FixtureGate {
    /// Block until this process is the only review-fixture gate.
    ///
    /// `std::fs::File::lock` is `flock(2)` on unix: a second handle — even in
    /// the SAME process — blocks, which is what serializes two `#[tokio::test]`
    /// functions of one binary without `--test-threads=1`. The wait is a plain
    /// blocking syscall by design: the gate tests are heavyweight, they do not
    /// run on a shared production runtime, and a lock is not cancellable.
    pub fn acquire() -> Result<Self, String> {
        let dir = gate_state_dir();
        std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
        let path = gate_lock_path();
        let file = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .map_err(|e| format!("open gate lock {}: {e}", path.display()))?;
        file.lock()
            .map_err(|e| format!("acquire gate lock {}: {e}", path.display()))?;
        Ok(Self { file })
    }
}

impl Drop for FixtureGate {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
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
    // mirrors every `write_output` call in the pipeline).
    let produced = report
        .files
        .iter()
        .filter_map(|file| {
            file.path
                .strip_prefix(&output_dir)
                .ok()
                .map(Path::to_path_buf)
        })
        .collect();
    Ok(produced)
}

/// Provision the review fixture (issue #694): from clean, under the gate.
///
/// 1. delete every file the previous provision produced (see
///    [`produced_ledger_path`]) so no artifact of an older generation can be
///    discovered as this run's;
/// 2. regenerate with [`regenerate_fixture`];
/// 3. record the new produced set.
///
/// It is a METHOD on the guard, not a free function, because the caller must
/// hold the gate for its whole body (the gates BUILD in this tree after
/// provisioning), and two `flock`s on the same file from one process would
/// deadlock — the signature makes it impossible to provision without it.
impl FixtureGate {
    pub async fn provision(&self) -> Result<(), String> {
        let output_dir = fixture_dir();
        sweep_previous_provision(&output_dir);
        let produced = regenerate_fixture().await?;
        record_provision(&produced)
    }
}

/// Delete the files the LAST provision recorded, and any directory that this
/// leaves empty.
///
/// Only files this recipe itself wrote can be in the ledger, so committed
/// fixture content (`ui/forms/`, `testkit/`, `codegraph-ops.toml`, the
/// retired-era trees) and gate-owned stubs are structurally safe — no
/// allowlist to drift. A missing ledger (first run, a fresh checkout, or a tree
/// just rewritten by `grafeo_candidate_inspect_output`) means there is nothing
/// to sweep, which is correct: the committed tree is the source of truth until
/// this recipe has produced a generation of its own.
///
/// The guarantee's boundary, stated so nobody reads it as more than it is: it
/// covers exactly the #688 class — a file a PREVIOUS generation of this recipe
/// emitted and the current one does not (a renumbered contract, a renamed
/// module). A file this recipe never produced (the flag-OFF recipe's output, a
/// hand edit) is not swept; the gates' documented flag-switch sweeps (the
/// retired stores, the shell routes) still own those.
fn sweep_previous_provision(output_dir: &Path) {
    let Ok(ledger) = std::fs::read_to_string(produced_ledger_path()) else {
        return;
    };
    let removed = sweep_listed(output_dir, &ledger);
    if removed > 0 {
        eprintln!(
            "provision: swept {removed} file(s) written by the previous generation of {}",
            output_dir.display()
        );
    }
}

/// [`sweep_previous_provision`] over an explicit ledger body (pure over the
/// filesystem, so the #694 rule is unit-testable without a 20-minute gate).
/// Returns how many files it removed.
fn sweep_listed(output_dir: &Path, ledger: &str) -> usize {
    let mut removed = 0usize;
    let mut touched_dirs: Vec<PathBuf> = Vec::new();
    for rel in ledger.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let Some(rel_path) = safe_relative(rel) else {
            // A ledger written by something else must never steer a delete
            // outside the fixture.
            continue;
        };
        let path = output_dir.join(&rel_path);
        if std::fs::remove_file(&path).is_ok() {
            removed += 1;
            if let Some(parent) = rel_path.parent() {
                touched_dirs.push(parent.to_path_buf());
            }
        }
    }
    // Deepest first so a whole retired tree collapses rather than leaving a
    // chain of empty directories for the next generation to inherit.
    touched_dirs.sort_by_key(|dir| std::cmp::Reverse(dir.components().count()));
    touched_dirs.dedup();
    for dir in &touched_dirs {
        let path = output_dir.join(dir);
        // `remove_dir` refuses non-empty directories: a dir still holding
        // committed or gate-owned content stays.
        let _ = std::fs::remove_dir(&path);
    }
    removed
}

/// A ledger entry is only ever a plain relative path inside the fixture.
fn safe_relative(rel: &str) -> Option<PathBuf> {
    let path = Path::new(rel);
    if path.is_absolute() {
        return None;
    }
    path.components()
        .all(|component| matches!(component, Component::Normal(_)))
        .then(|| path.to_path_buf())
}

/// The ledger body for a produced set: sorted, de-duplicated, `/`-separated,
/// one per line. Only paths inside the fixture survive [`safe_relative`].
fn ledger_text(produced: &[PathBuf]) -> String {
    let mut rels: Vec<String> = produced
        .iter()
        .filter(|path| safe_relative(&path.to_string_lossy()).is_some())
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .collect();
    rels.sort();
    rels.dedup();
    format!("{}\n", rels.join("\n"))
}

fn record_provision(produced: &[PathBuf]) -> Result<(), String> {
    let path = produced_ledger_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    std::fs::write(&path, ledger_text(produced))
        .map_err(|e| format!("write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, rel: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "x").unwrap();
    }

    /// The #694 from-clean rule: last generation's files go, everything the
    /// recipe did not write (committed extras, gate-owned stubs) stays, and a
    /// directory the sweep empties is pruned.
    #[test]
    fn the_sweep_deletes_only_the_previous_generations_files() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(root, "src/domain/x/mod.rs");
        write(root, "hurl/11_payroll_pay_run_actions.hurl");
        write(root, "ui/forms/text.svelte"); // consumer-authored
        write(root, "testkit/Cargo.toml"); // committed extra
        write(root, "codegraph-ops.toml"); // committed manifest

        let ledger = "src/domain/x/mod.rs\nhurl/11_payroll_pay_run_actions.hurl\n";
        assert_eq!(sweep_listed(root, ledger), 2);

        assert!(!root.join("src/domain/x/mod.rs").exists());
        assert!(!root.join("hurl/11_payroll_pay_run_actions.hurl").exists());
        assert!(!root.join("hurl").exists(), "an emptied dir is pruned");
        assert!(!root.join("src/domain/x").exists());
        assert!(root.join("ui/forms/text.svelte").is_file());
        assert!(root.join("testkit/Cargo.toml").is_file());
        assert!(root.join("codegraph-ops.toml").is_file());
    }

    /// A ledger entry that escapes the fixture (a hand-edited or foreign
    /// ledger) is ignored — never a delete outside the tree.
    #[test]
    fn the_sweep_refuses_paths_outside_the_fixture() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = tmp.path().parent().unwrap().join("outside-694.txt");
        std::fs::write(&outside, "keep me").unwrap();

        let root = tmp.path().join("fixture");
        std::fs::create_dir_all(&root).unwrap();
        let ledger = format!("../outside-694.txt\n{}\n", outside.to_string_lossy());
        assert_eq!(sweep_listed(&root, &ledger), 0);
        assert!(
            outside.is_file(),
            "a path outside the fixture is never removed"
        );
        let _ = std::fs::remove_file(&outside);
    }

    /// The ledger round-trips: recorded paths are what the next sweep removes,
    /// sorted and de-duplicated.
    #[test]
    fn the_ledger_round_trips_a_produced_set() {
        let produced = vec![
            PathBuf::from("src/b.rs"),
            PathBuf::from("src/a.rs"),
            PathBuf::from("src/a.rs"),
        ];
        assert_eq!(ledger_text(&produced), "src/a.rs\nsrc/b.rs\n");
        assert_eq!(safe_relative("src/a.rs"), Some(PathBuf::from("src/a.rs")));
        assert_eq!(safe_relative("/etc/passwd"), None);
        assert_eq!(safe_relative("../escape.rs"), None);
    }
}
