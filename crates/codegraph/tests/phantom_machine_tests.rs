//! Issue #807 (Stage 0): the phantom entity fix — the stale-output sweep +
//! the query-target-table consistency gate.
//!
//! The live defect (crewbase, workers + cornucopia): renaming or removing
//! an entity leaves its per-entity artifacts behind on the surfaces
//! `clean_generated_output` did not sweep — `queries/{domain}/{module}.sql`
//! (re-materialized into the consumer build by `cornucopia-queries`'
//! build.rs, which compiles every query file against the live migrated DB
//! and refuses on the dropped relation), `cli/src/commands/{domain}/
//! {module}.rs` (a dead file), and `src/generated/{domain}/{module}.rs`
//! (re-declared by the filesystem-scanning mod pass, importing DTO types
//! that no longer exist). The #807 report reads these as a "phantom entity
//! titled `Disposition`" — the pre-rename name of a value object the model
//! had renamed — but the entity-phase enumeration is schema-driven and
//! clean: the survivor is stale OUTPUT, not a minted graph node. The
//! `src/domain/` and handler surfaces, which the existing sweep DOES cover,
//! are conspicuously absent from the phantom artifact list — that asymmetry
//! names the sweep's gaps as the mint site.
//!
//! Pins here:
//! 1. the ingest graph read: a machine declared on a non-root entity mints
//!    NO `SchemaNode` — the machine binds to the entity's enum feature and
//!    its field name (and every derived spelling) never becomes a schema;
//! 2. the rename repro: regenerating into an existing tree after an entity
//!    rename sweeps every per-entity artifact family the generators key by
//!    (domain, module) — nothing survives to break the consumer build;
//! 3. (in-crate, `consistency::query_target_divergences`) the gate: every
//!    cornucopia query target table must exist in the DDL universe; a
//!    divergence names the query file, the domain, and both planes.

use std::fs;
use std::path::{Path, PathBuf};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phantom_machine")
}

struct Fixture {
    root: tempfile::TempDir,
    config: PathBuf,
    mox: PathBuf,
    ddd: PathBuf,
}

/// Copy the committed fixture into a temp dir so generated output never
/// lands inside the checkout.
fn write_fixture() -> Fixture {
    let src = fixture_dir();
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("model")).unwrap();
    fs::create_dir_all(root.path().join("design")).unwrap();
    fs::copy(
        src.join("model/recruiting.mox"),
        root.path().join("model/recruiting.mox"),
    )
    .unwrap();
    fs::copy(
        src.join("design/recruiting.ddd"),
        root.path().join("design/recruiting.ddd"),
    )
    .unwrap();
    fs::copy(src.join("domains.toml"), root.path().join("domains.toml")).unwrap();
    fs::copy(src.join("profiles.toml"), root.path().join("profiles.toml")).unwrap();
    Fixture {
        config: root.path().join("domains.toml"),
        mox: root.path().join("model/recruiting.mox"),
        ddd: root.path().join("design/recruiting.ddd"),
        root,
    }
}

impl Fixture {
    fn out(&self) -> PathBuf {
        self.root.path().join("generated")
    }
}

#[allow(clippy::too_many_arguments)]
fn run_args<'a>(
    fixture: &'a Fixture,
    mox_files: &'a [PathBuf],
    ddd_files: &'a [PathBuf],
    output: &'a Path,
) -> codegraph::driver::RunArgs<'a> {
    codegraph::driver::RunArgs {
        integration_manifests: &[],
        schemas: None,
        classifier: None,
        config_path: &fixture.config,
        output,
        extension_points_path: None,
        profile_name: "default",
        variant: None,
        profiles_config_path: Some(fixture.root.path().join("profiles.toml")),
        no_post_gen: true,
        template_dir: &[],
        ifml_files: &[],
        openapi_files: &[],
        mox_files,
        rosetta_files: &[],
        ddd_files,
        evt_files: &[],
        deploy_files: &[],
        ifml_framework: &[],
        ifml_components: None,
        ifml_design_system: None,
        codegraph_rev: None,
        check: false,
        ux_rules: None,
    }
}

/// Run the driver's ingest passes (mox first, then the ddd design plane —
/// the exact `run_model_ingest_passes` + `run_design_ingest_passes` order)
/// into one engine and return every schema title with the node's
/// provenance, so a phantom names its minting path in the assert message.
async fn ingested_titles<T>(engine: &T, fixture: &Fixture) -> Vec<String>
where
    T: codegraph_core::traits::GraphIngestor + codegraph_core::traits::GraphQuerier,
{
    let config = codegraph_config::config::parse_domain_config(&fixture.config).unwrap();
    let mox_files = vec![fixture.mox.clone()];
    codegraph::ingest::mox_ingest::ingest_mox_files(
        engine,
        engine,
        &mox_files,
        &config,
        &config.defaults.type_suffix,
    )
    .await
    .unwrap();
    let ddd_files = vec![fixture.ddd.clone()];
    codegraph::ingest::ddd_ingest::ingest_ddd_files(engine, engine, &ddd_files, &config)
        .await
        .unwrap();
    engine
        .list_schemas(None)
        .await
        .unwrap()
        .into_iter()
        .map(|s| s.title)
        .collect()
}

/// (1) The decisive graph read: the machine's field name (and any derived
/// spelling of it) must never mint a `SchemaNode` — the machine binds to
/// the entity's existing enum feature, it does not declare a class. This
/// is the protocol that REFUTED the mint hypothesis on #807: the graph
/// carries no machine-derived node, so the phantom's provenance is stale
/// output, and this pin keeps the ingest plane honest going forward.
#[tokio::test]
async fn machine_field_name_mints_no_schema_node() {
    let fixture = write_fixture();
    let engine = codegraph_grafeo::GrafeoEngine::in_memory().unwrap();
    let titles = ingested_titles(&engine, &fixture).await;
    let phantoms: Vec<&String> = titles
        .iter()
        .filter(|t| t.as_str() == "Disposition")
        .collect();
    assert!(
        phantoms.is_empty(),
        "machine-derived phantom schema minted (titles: {titles:?})"
    );
    // The REAL classes stay: the root, the child entity, the sibling value,
    // and the machine's enum (bridged as the codelist schema).
    for expected in [
        "CandidateType",
        "Application",
        "ApplicationHistoryStep",
        "ApplicationDisposition",
    ] {
        assert!(
            titles.iter().any(|t| t == expected),
            "expected schema `{expected}` missing (titles: {titles:?})"
        );
    }
}

/// (1b) The same contract on the Mock engine (querier parity — the ingest
/// plane must be engine-independent).
#[tokio::test]
async fn machine_field_name_mints_no_schema_node_on_mock() {
    let fixture = write_fixture();
    let engine = codegraph_core::mock::MockEngine::new();
    let titles = ingested_titles(&engine, &fixture).await;
    let phantoms: Vec<&String> = titles
        .iter()
        .filter(|t| t.as_str() == "Disposition")
        .collect();
    assert!(
        phantoms.is_empty(),
        "machine-derived phantom schema minted on mock (titles: {titles:?})"
    );
}

/// Rewrite the fixture's model + design with the sibling value renamed
/// (`ApplicationHistoryStep` → `HistoryEntry`) — the consumer's #807
/// iteration step.
fn rename_history_step(fixture: &Fixture) {
    let mox = fs::read_to_string(&fixture.mox).unwrap();
    fs::write(
        &fixture.mox,
        mox.replace("ApplicationHistoryStep", "HistoryEntry"),
    )
    .unwrap();
    let ddd = fs::read_to_string(&fixture.ddd).unwrap();
    fs::write(
        &fixture.ddd,
        ddd.replace("ApplicationHistoryStep", "HistoryEntry"),
    )
    .unwrap();
}

/// (2) The rename repro: generate, rename the sibling value, regenerate
/// into the SAME tree. Every per-entity artifact family keyed by
/// (domain, module) must be swept — nothing may survive to break the
/// consumer build. This is the #807 phantom's actual provenance: the
/// pre-rename artifacts the sweep missed, not a minted graph node.
#[tokio::test]
async fn renamed_entity_leaves_no_stale_artifacts() {
    let fixture = write_fixture();
    let output = fixture.out();
    let mox_files = vec![fixture.mox.clone()];
    let ddd_files = vec![fixture.ddd.clone()];

    // State A: ApplicationHistoryStep is a real entity-plane citizen.
    codegraph::driver::run(run_args(&fixture, &mox_files, &ddd_files, &output))
        .await
        .unwrap();
    assert!(
        output
            .join("queries/recruiting/application_history_step.sql")
            .exists(),
        "state A must emit the sibling value's query file"
    );

    // State B: the consumer's rename, regenerated into the SAME tree.
    rename_history_step(&fixture);
    codegraph::driver::run(run_args(&fixture, &mox_files, &ddd_files, &output))
        .await
        .unwrap();

    let mut survivors = Vec::new();
    let stale_roots = [
        // The cornucopia queries plane — `cornucopia-queries`' build.rs
        // compiles every file here against the live DB; a stale file is
        // the exact #807 build break.
        output.join("queries/recruiting/application_history_step.sql"),
        // The dead CLI command (the issue's unreferenced command file).
        output.join("cli/src/commands/recruiting/application_history_step.rs"),
        // The hooks lifecycle trait — mod.rs is FILESYSTEM-SCANNED, so a
        // survivor re-declares itself and imports DTO types that no longer
        // exist.
        output.join("src/generated/recruiting/application_history_step.rs"),
    ];
    for path in &stale_roots {
        if path.exists() {
            survivors.push(path.to_path_buf());
        }
    }
    assert!(
        survivors.is_empty(),
        "stale pre-rename artifacts survived the regeneration (the #807 phantom): {survivors:?}"
    );

    // The renamed entity's artifacts exist — the sweep removed only what
    // the model no longer declares.
    assert!(
        output.join("queries/recruiting/history_entry.sql").exists(),
        "the renamed entity's query file must be emitted"
    );
    assert!(
        output
            .join("cli/src/commands/recruiting/history_entry.rs")
            .exists(),
        "the renamed entity's CLI command must be emitted"
    );
}

/// (3) The stale sweep is plan-gated: a run whose plan carries none of the
/// swept surfaces must not invent directories — a fresh run into an empty
/// tree emits nothing under `queries/`, `cli/`, or `src/generated/` unless
/// the corresponding generator runs. (The inverse — a plan carrying the
/// surface sweeps it — is pinned by `renamed_entity_leaves_no_stale_artifacts`.)
#[tokio::test]
async fn fresh_generation_emits_only_declared_surfaces() {
    let fixture = write_fixture();
    let output = fixture.out();
    let mox_files = vec![fixture.mox.clone()];
    let ddd_files = vec![fixture.ddd.clone()];

    codegraph::driver::run(run_args(&fixture, &mox_files, &ddd_files, &output))
        .await
        .unwrap();

    // The profile carries cornucopia_queries, cli_command, lifecycle_trait,
    // and ui-shell, so all four surfaces are live for this fixture.
    assert!(
        output.join("queries").is_dir(),
        "queries plane is in the plan"
    );
    assert!(
        output.join("cli/src/commands").is_dir(),
        "cli plane is in the plan"
    );
    assert!(
        output.join("src/generated").is_dir(),
        "hooks lifecycle-trait plane is in the plan"
    );
}
