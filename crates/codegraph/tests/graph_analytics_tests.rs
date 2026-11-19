//! Issue #673 — the upstream semantic graph + graph-query analytics as a
//! validation plane. Node-free, DB-free pins over
//! `crates/codegraph/tests/fixtures/graph_analytics/`:
//!
//! - separation-of-duty over a COMMITTED policy artifact — `.json`
//!   policy imports load through `ActorModel::from_json` with no
//!   compile, so a violation the `.actor` compile would reject enters
//!   the graph unchecked and only the sweep can flag it;
//! - dead capabilities (and latent deliberately NOT reported);
//! - monotonicity gaps on `extends` edges;
//! - reachability as the doctor passthrough query (reachable ≠ finding,
//!   unreachable = finding);
//! - the artifact-alone contract: analysis runs over the SERIALIZED
//!   `GraphModel` (`to_json` → `from_json`), and `--graph-artifact` mode
//!   drives the doctor verdict without recompiling sources;
//! - the doctor contract: findings are soft warnings by default and hard
//!   failures under `--fail-on-findings`.
//!
//! Adapted from upstream's analytics conformance policy
//! (`pherolang tests/conformance/graph/app.actor`), which exercises every
//! outcome the four analytics distinguish.

#[path = "test_framework/mod.rs"]
mod test_framework;

use std::path::{Path, PathBuf};

use codegraph::doctor_graph::{
    analyze_sweeps, build_model_from, gather_artifacts, load_graph_artifact, parse_reach_spec,
    run_reachability,
};
use codegraph::init::commands::{DoctorArgs, cmd_doctor};
use phero_ir::graph::GraphModel;

const FIXTURE: &str = "tests/fixtures/graph_analytics";

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(FIXTURE)
        .join(name)
}

fn copy_fixture(dir: &Path, name: &str) -> PathBuf {
    let from = fixture_path(name);
    let to = dir.join(name);
    std::fs::copy(&from, &to).unwrap_or_else(|e| panic!("copy {name}: {e}"));
    to
}

// ---------------------------------------------------------------------------
// Analytics over the gathered artifacts
// ---------------------------------------------------------------------------

/// separation-of-duty across MERGED policies: each `.actor` file compiles
/// alone (the actor compiler only rejects a single-file `never_both`
/// violation), the union violates, and only the graph sweep can see it.
#[test]
fn sod_violation_found_across_merged_policies() {
    let gather = gather_artifacts(
        &[fixture_path("app.mox")],
        &[],
        &[],
        &[],
        &[fixture_path("sod.ifml")],
    );
    let model = build_model_from(&gather).expect("graph model builds");
    let findings = analyze_sweeps(&model);

    assert_eq!(findings.len(), 1, "exactly the SoD violation: {findings:?}");
    let finding = &findings[0];
    assert_eq!(finding.analytic, "separationOfDuty");
    assert!(
        finding.subject.contains("Finance"),
        "subject names the violating actor: {}",
        finding.subject
    );
    assert!(
        finding.detail.contains("ReadTicket") && finding.detail.contains("ResolveTicket"),
        "detail names both capabilities: {}",
        finding.detail
    );
    assert!(
        finding.detail.contains("never_both"),
        "detail says the pair was declared: {}",
        finding.detail
    );
}

/// The dead capability is a finding; the latent one deliberately is not;
/// the monotonicity gap is a finding naming the missing capability.
#[test]
fn dead_and_monotonicity_found_and_latent_not_reported() {
    let gather = gather_artifacts(
        &[fixture_path("app.mox")],
        &[],
        &[],
        &[],
        &[fixture_path("gaps.ifml")],
    );
    let model = build_model_from(&gather).expect("graph model builds");
    let findings = analyze_sweeps(&model);

    assert_eq!(findings.len(), 2, "dead + monotonicity only: {findings:?}");

    let dead = findings
        .iter()
        .find(|f| f.analytic == "deadCapabilities")
        .expect("a dead-capability finding");
    assert!(
        dead.subject.contains("ResolveTicket"),
        "the dead capability is ResolveTicket: {}",
        dead.subject
    );
    assert!(
        dead.detail.contains("Finance"),
        "the blocker is Finance: {}",
        dead.detail
    );

    let mono = findings
        .iter()
        .find(|f| f.analytic == "monotonicity")
        .expect("a monotonicity finding");
    assert!(
        mono.subject.contains("Manager"),
        "the child actor is Manager: {}",
        mono.subject
    );
    assert!(
        mono.detail.contains("Agent") && mono.detail.contains("UpdateTicket"),
        "the gap names the parent and the missing capability: {}",
        mono.detail
    );

    assert!(
        !findings
            .iter()
            .any(|f| f.subject.contains("EscalateRefund")),
        "latent (declared, never granted) is never a finding"
    );
}

/// A clean policy produces zero findings.
#[test]
fn clean_policy_yields_no_findings() {
    let gather = gather_artifacts(
        &[fixture_path("app.mox")],
        &[],
        &[],
        &[],
        &[fixture_path("clean.ifml")],
    );
    let model = build_model_from(&gather).expect("graph model builds");
    assert!(
        analyze_sweeps(&model).is_empty(),
        "a clean policy has nothing to report"
    );
}

/// A mox-only model (no policy artifacts) analyzes to zero findings —
/// the sweeps have no capabilities to judge.
#[test]
fn mox_only_model_analyzes_to_zero_findings() {
    let gather = gather_artifacts(&[fixture_path("app.mox")], &[], &[], &[], &[]);
    let model = build_model_from(&gather).expect("graph model builds");
    assert!(analyze_sweeps(&model).is_empty());
}

// ---------------------------------------------------------------------------
// The artifact-alone contract
// ---------------------------------------------------------------------------

/// Analysis runs from the artifact alone: the serialized `GraphModel`
/// reloaded through `from_json` yields byte-equal findings, with no
/// sources and no recompile.
#[test]
fn analysis_runs_from_the_serialized_artifact_alone() {
    let gather = gather_artifacts(
        &[fixture_path("app.mox")],
        &[],
        &[],
        &[],
        &[fixture_path("sod.ifml")],
    );
    let model = build_model_from(&gather).expect("graph model builds");
    let expected = analyze_sweeps(&model);

    let json = model.to_json_pretty().expect("serialize");
    let reloaded = GraphModel::from_json(&json).expect("deserialize");
    assert_eq!(analyze_sweeps(&reloaded), expected);
}

/// `load_graph_artifact` reads a committed artifact from disk and the
/// analytics over it match the in-memory build.
#[test]
fn committed_artifact_round_trips_through_disk() {
    let gather = gather_artifacts(
        &[fixture_path("app.mox")],
        &[],
        &[],
        &[],
        &[fixture_path("sod.ifml")],
    );
    let model = build_model_from(&gather).expect("graph model builds");
    let expected = analyze_sweeps(&model);

    let dir = tempfile::TempDir::new().expect("temp dir");
    let path = dir.path().join("graph.json");
    std::fs::write(&path, model.to_json_pretty().expect("serialize")).expect("write");

    let reloaded = load_graph_artifact(&path).expect("artifact loads");
    assert_eq!(analyze_sweeps(&reloaded), expected);
}

// ---------------------------------------------------------------------------
// The reachability passthrough
// ---------------------------------------------------------------------------

/// A reachable target answers with no finding.
#[test]
fn reachability_reachable_target_is_no_finding() {
    let gather = gather_artifacts(
        &[fixture_path("app.mox")],
        &[],
        &[],
        &[],
        &[fixture_path("clean.ifml")],
    );
    let model = build_model_from(&gather).expect("graph model builds");

    let query = parse_reach_spec("from-actor=Customer,to=Ticket").expect("spec parses");
    let outcome = run_reachability(&model, &query).expect("analytics answer");
    assert!(outcome.reachable, "Ticket is the capability's own class");
    assert!(
        outcome.findings.is_empty(),
        "reachable is never a finding: {:?}",
        outcome.findings
    );
    assert!(
        outcome
            .envelope_json
            .contains("\"query\": \"reachability\""),
        "the upstream envelope is carried verbatim: {}",
        outcome.envelope_json
    );
}

/// An unreachable target IS a finding — the thing asked for is not there.
#[test]
fn reachability_unreachable_target_is_a_finding() {
    let gather = gather_artifacts(
        &[fixture_path("app.mox")],
        &[],
        &[],
        &[],
        &[fixture_path("clean.ifml")],
    );
    let model = build_model_from(&gather).expect("graph model builds");

    let query = parse_reach_spec("from-actor=Customer,to=Patron").expect("spec parses");
    let outcome = run_reachability(&model, &query).expect("analytics answer");
    assert!(!outcome.reachable, "Patron has no connecting relation");
    assert_eq!(outcome.findings.len(), 1);
    assert_eq!(outcome.findings[0].analytic, "reachability");
}

/// The spec grammar is strict: exactly one start, a target, unknown keys
/// rejected.
#[test]
fn malformed_reach_specs_are_parse_errors() {
    assert!(parse_reach_spec("to=Ticket").is_err(), "no start");
    assert!(
        parse_reach_spec("from-actor=A,from-capability=B,to=T").is_err(),
        "two starts"
    );
    assert!(parse_reach_spec("from-actor=A").is_err(), "no target");
    assert!(
        parse_reach_spec("from-actor=A,to=T,bogus=1").is_err(),
        "unknown key"
    );
    assert!(
        parse_reach_spec("from-actor").is_err(),
        "missing = is not a key"
    );
}

/// An unresolvable reference is an analytics ERROR (a malformed request,
/// never a silent answer).
#[test]
fn unresolvable_reach_reference_is_an_error() {
    let gather = gather_artifacts(
        &[fixture_path("app.mox")],
        &[],
        &[],
        &[],
        &[fixture_path("clean.ifml")],
    );
    let model = build_model_from(&gather).expect("graph model builds");

    let query = parse_reach_spec("from-actor=Nobody,to=Ticket").expect("spec parses");
    let error = run_reachability(&model, &query).expect_err("unresolvable actor errors");
    assert!(
        error.contains("Nobody"),
        "the error names the unresolved reference: {error}"
    );
}

// ---------------------------------------------------------------------------
// The doctor contract
// ---------------------------------------------------------------------------

/// `--fail-on-findings` makes a policy finding a HARD doctor failure;
/// without the flag the same run passes with the finding as a soft
/// warning. The stable exit-code contract.
#[test]
fn fail_on_findings_makes_sod_finding_hard() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let mox = copy_fixture(dir.path(), "app.mox");
    let ifml = copy_fixture(dir.path(), "sod.ifml");
    for name in ["sod.json", "domains.toml"] {
        copy_fixture(dir.path(), name);
    }

    let flag_on = DoctorArgs {
        config: dir.path().join("domains.toml"),
        schemas: None,
        classifier: None,
        profiles_config: None,
        mox_files: vec![mox.clone()],
        rosetta_files: Vec::new(),
        ddd_files: Vec::new(),
        evt_files: Vec::new(),
        deploy_files: Vec::new(),
        ifml_files: vec![ifml.clone()],
        extension_points: None,
        fail_on_findings: true,
        graph_artifact: None,
        emit_graph: None,
        graph_reachability: Vec::new(),
    };
    let flag_off = DoctorArgs {
        fail_on_findings: false,
        ..flag_on.clone()
    };

    assert!(
        cmd_doctor(&flag_on).is_err(),
        "--fail-on-findings turns the SoD finding into a hard failure"
    );
    match cmd_doctor(&flag_off) {
        Ok(summary) => assert!(
            summary.soft_warnings >= 1,
            "flag-off the finding is a soft warning: {summary:?}"
        ),
        Err(e) => panic!("flag-off doctor passes: {e}"),
    }
}

/// `--graph-artifact` mode drives the verdict from the COMMITTED artifact
/// alone: a clean artifact passes `--fail-on-findings` even though the
/// on-disk sources carry the violation — the analysis did not recompile
/// them.
#[test]
fn graph_artifact_mode_needs_no_recompile() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let mox = copy_fixture(dir.path(), "app.mox");
    let ifml = copy_fixture(dir.path(), "sod.ifml");
    for name in ["sod.json", "domains.toml"] {
        copy_fixture(dir.path(), name);
    }

    // A CLEAN artifact built from the clean fixture, committed beside
    // violating sources.
    let clean_gather = gather_artifacts(
        &[fixture_path("app.mox")],
        &[],
        &[],
        &[],
        &[fixture_path("clean.ifml")],
    );
    let clean_model = build_model_from(&clean_gather).expect("clean model builds");
    let artifact = dir.path().join("committed-graph.json");
    std::fs::write(&artifact, clean_model.to_json_pretty().expect("serialize"))
        .expect("write artifact");

    let args = DoctorArgs {
        config: dir.path().join("domains.toml"),
        schemas: None,
        classifier: None,
        profiles_config: None,
        mox_files: vec![mox],
        rosetta_files: Vec::new(),
        ddd_files: Vec::new(),
        evt_files: Vec::new(),
        deploy_files: Vec::new(),
        ifml_files: vec![ifml],
        extension_points: None,
        fail_on_findings: true,
        graph_artifact: Some(artifact),
        emit_graph: None,
        graph_reachability: Vec::new(),
    };

    match cmd_doctor(&args) {
        Ok(_) => {}
        Err(e) => panic!("the committed artifact decided, not the sources (no recompile): {e}"),
    }
}

/// `--emit-graph` writes the artifact to the given path.
#[test]
fn emit_graph_writes_the_artifact() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let mox = copy_fixture(dir.path(), "app.mox");
    let ifml = copy_fixture(dir.path(), "clean.ifml");
    for name in ["clean.actor", "domains.toml"] {
        copy_fixture(dir.path(), name);
    }

    let out = dir.path().join("emitted.json");
    let args = DoctorArgs {
        config: dir.path().join("domains.toml"),
        schemas: None,
        classifier: None,
        profiles_config: None,
        mox_files: vec![mox],
        rosetta_files: Vec::new(),
        ddd_files: Vec::new(),
        evt_files: Vec::new(),
        deploy_files: Vec::new(),
        ifml_files: vec![ifml],
        extension_points: None,
        fail_on_findings: false,
        graph_artifact: None,
        emit_graph: Some(out.clone()),
        graph_reachability: Vec::new(),
    };

    cmd_doctor(&args).expect("doctor passes");
    let written = std::fs::read_to_string(&out).expect("artifact written");
    assert!(
        written.contains("\"formatVersion\": 1"),
        "the artifact carries the graph format version: {written}"
    );
}

/// A `--graph-reachability` query over unreachable data is a finding
/// under the same severity contract as the sweeps.
#[test]
fn doctor_reachability_finding_follows_the_severity_contract() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let mox = copy_fixture(dir.path(), "app.mox");
    let ifml = copy_fixture(dir.path(), "clean.ifml");
    for name in ["clean.actor", "domains.toml"] {
        copy_fixture(dir.path(), name);
    }

    let base = DoctorArgs {
        config: dir.path().join("domains.toml"),
        schemas: None,
        classifier: None,
        profiles_config: None,
        mox_files: vec![mox.clone()],
        rosetta_files: Vec::new(),
        ddd_files: Vec::new(),
        evt_files: Vec::new(),
        deploy_files: Vec::new(),
        ifml_files: vec![ifml.clone()],
        extension_points: None,
        fail_on_findings: false,
        graph_artifact: None,
        emit_graph: None,
        graph_reachability: Vec::new(),
    };
    let query = "from-actor=Customer,to=Patron".to_string();
    let flag_on = DoctorArgs {
        fail_on_findings: true,
        graph_reachability: vec![query.clone()],
        ..base.clone()
    };
    let flag_off = DoctorArgs {
        graph_reachability: vec![query],
        ..base
    };

    assert!(
        cmd_doctor(&flag_on).is_err(),
        "an unreachable target fails under --fail-on-findings"
    );
    assert!(
        cmd_doctor(&flag_off).is_ok(),
        "flag-off the reachability finding is advisory"
    );
}

/// A malformed `--graph-reachability` spec is a hard doctor error — a
/// malformed request, never a silent answer.
#[test]
fn doctor_malformed_reach_spec_is_a_hard_error() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let mox = copy_fixture(dir.path(), "app.mox");
    let ifml = copy_fixture(dir.path(), "clean.ifml");
    for name in ["clean.actor", "domains.toml"] {
        copy_fixture(dir.path(), name);
    }

    let args = DoctorArgs {
        config: dir.path().join("domains.toml"),
        schemas: None,
        classifier: None,
        profiles_config: None,
        mox_files: vec![mox],
        rosetta_files: Vec::new(),
        ddd_files: Vec::new(),
        evt_files: Vec::new(),
        deploy_files: Vec::new(),
        ifml_files: vec![ifml],
        extension_points: None,
        fail_on_findings: false,
        graph_artifact: None,
        emit_graph: None,
        graph_reachability: vec!["to=Ticket".to_string()],
    };

    assert!(
        cmd_doctor(&args).is_err(),
        "a spec with no start is a hard error"
    );
}
