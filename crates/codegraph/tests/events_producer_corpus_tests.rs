//! Events-producer corpus (issue #820 slice c): the catalogue marks and the
//! consumer survey shapes.
//!
//! Two legs, because a flag-ON validation aborts before generation:
//!
//! (a) **planner/validation** over the FULL corpus — every class-C shape
//!     yields its exact documented sentence (verbatim from
//!     `docs/events-producer.md`), carries NO plan (never a partially bound
//!     payload), and the flag-ON validation pass reports them as hard errors
//!     while a flag-OFF / plan-less run still succeeds;
//! (b) **generation** over a CLEAN A/B subset (derived here, same model and
//!     contracts) — the emitted commands carry the in-transaction publish and
//!     the generated `docs/events.md` carries the exact
//!     `Runtime publication: published at runtime by …` line for every A/B
//!     event plus the unbound line for the declared-but-unemitted ones. That
//!     line is the drift pin between the planner plane and the catalogue
//!     plane; the flag-OFF tree is held to the same document minus the marks.
//!
//! Node-free (MockEngine/ingest, the `events_producer_tests` shape).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use codegraph_backend::{Backend, BackendConfig, create_backend};
use codegraph_generate::db::dialect::DatabaseTarget;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/events_producer_corpus")
        .join(name)
}

/// The corpus bounded context. `ShipmentType.status` is workflow-managed (a
/// valid consumer shape); the planner reads the DTO plane's own surface, so
/// the Update DTO excluding it is what class A's clean payload relies on.
const DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.corpus]
label = "Corpus"
schema_dir = "corpus"
postgres_schema = "corpus"

[domains.corpus.entity_config.ShipmentType.workflow]
status_field = "status"
states = ["Draft", "Booked", "Cancelled"]
initial_state = "Draft"
terminal_states = ["Cancelled"]
"#;

/// The minimal profile both legs share; `events_producer` is the only
/// difference between the flag-ON and flag-OFF runs.
const PROFILE_TEMPLATE: &str = r#"
[profiles.default.meta]
name = "default"
version = "1.0.0"
description = ""

[profiles.default.features]
events_producer = {flag}

[profiles.default.api]
generators = [
    "ddl", "sea_orm_entity", "dto", "domain_types_dto", "repository",
    "command", "query", "handler", "router", "errors", "evt_events",
]
"#;

/// The CLEAN A/B designs: the same model and contracts, with only the
/// publishable shapes bound. Class A (a plain patch), class B (a bound
/// transition with the clock/state/correlation facts), and the
/// pluralization shapes the catalogue's `arms:`/`queues:` forms key on —
/// one event from two patch ops on two channels, one event from a transition
/// and a patch.
const CLEAN_DDD: &str = r#"import "corpus.mox";

application Corpus {
    base nz.example.corpus

    module corpus {
        entity ShipmentType scaffold repository ShipmentRepository {
            findById;
            findAll;
            save;
            delete;
        }

        entity JobType scaffold repository JobRepository {
            findById;
            findAll;
            save;
            delete;
        }

        machine jobState: JobState {
            initial Queued;
            Queued -> Running via Start;
            Running -> Done via Finish;
            terminal Done;
        }

        service ShipmentService {
            inject ShipmentRepository;
            book => ShipmentRepository.save emits ShipmentBooked;
            amend => ShipmentRepository.save emits ShipmentBooked;
        }

        service JobService {
            inject JobRepository;
            start => JobRepository.save emits JobStarted transition Start;
            touch => JobRepository.save emits JobTouched transition Start;
            retouch => JobRepository.save emits JobTouched;
        }
    }
}
"#;

/// Every class-C shape's documented sentence, verbatim from
/// `docs/events-producer.md` (declaration order: the planner walks the
/// service operations in authored order and the payload fields in
/// declaration order, so the first blocker per pair is stable).
const DOCUMENTED_REFUSALS: &[&str] = &[
    // R1 — the `_code`-stripped near miss.
    "event `ShipmentStatusRecorded` (operation `ShipmentService.recordStatus`) payload field \
     `statusCode` has no matching field on `ShipmentResponse`; nearest: `status` — rename the \
     payload field in the .evt contract or the class feature so the spellings agree",
    // R1 — nothing within edit distance 2.
    "event `ShipmentMystery` (operation `ShipmentService.recordMystery`) payload field `xyzzy` \
     has no matching field on `ShipmentResponse`; nearest: (none within edit distance 2) — \
     rename the payload field in the .evt contract or the class feature so the spellings agree",
    // R2 — a nullable source feeding a required payload field.
    "event `ShipmentNoted` (operation `ShipmentService.recordNote`) payload field `note` is \
     required but `ShipmentResponse.note` is nullable — declare the payload field optional or \
     make the source feature required (v1 refuses to invent a null policy)",
    // R3 — a contained-child-only source.
    "event `ParcelWeighed` (operation `ShipmentService.recordWeight`) payload field \
     `weightGrams` is sourced only from the contained collection `Shipment.parcels` — v1 \
     publishes record-compatible payloads only; lift the value onto the record or emit a \
     child-scoped event",
    // R9 — a transition fact on an entity with no machine.
    "event `ShipmentStateGuessed` (operation `ShipmentService.guessState`) payload field \
     `previousState` has no model source — v1 publishes record-compatible payloads only; add \
     the feature (or a context binding) to the model",
    // R4 — a `protected` op is a valid visibility, not a publisher.
    "event `ShipmentArchived` is bound to operation `ShipmentService.archive` which has no \
     runtime site (protected) — the event cannot fire; bind it to an executing operation",
    // R4 — the `findAll` seam.
    "event `ShipmentBrowsed` is bound to operation `ShipmentService.browse` which has no \
     runtime site (abstract) — the event cannot fire; bind it to an executing operation",
    // R7 — no coercion.
    "event `JobMislabelled` (operation `JobService.mislabel`) payload field `attempts` is \
     `String` but `JobResponse.attempts` is `i32` — v1 does not coerce the value types; align \
     the model",
];

/// The refused `(event, op)` pairs — no plan may exist for any of them (no
/// partially bound payload, no silently skipped emission).
const REFUSED_PAIRS: &[(&str, &str)] = &[
    ("ShipmentStatusRecorded", "recordStatus"),
    ("ShipmentMystery", "recordMystery"),
    ("ShipmentNoted", "recordNote"),
    ("ParcelWeighed", "recordWeight"),
    ("ShipmentStateGuessed", "guessState"),
    ("ShipmentArchived", "archive"),
    ("ShipmentBrowsed", "browse"),
    ("JobMislabelled", "mislabel"),
];

fn write_config(dir: &Path) -> PathBuf {
    let path = dir.join("domains.toml");
    fs::write(&path, DOMAINS_TOML).unwrap();
    path
}

fn write_profiles(dir: &Path, flag: bool) -> PathBuf {
    // Distinct file names: the two legs of the generation test coexist in
    // one temp dir, and the flag is the ONLY difference between them.
    let path = dir.join(if flag {
        "profiles-on.toml"
    } else {
        "profiles-off.toml"
    });
    let body = PROFILE_TEMPLATE.replace("{flag}", if flag { "true" } else { "false" });
    fs::write(&path, body).unwrap();
    path
}

/// Copy the model + contracts into `dir` and write the CLEAN designs beside
/// them (pherolang imports resolve relative to the declaring file).
fn stage_clean_project(dir: &Path) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let config = write_config(dir);
    fs::copy(fixture("corpus.mox"), dir.join("corpus.mox")).unwrap();
    fs::copy(fixture("corpus.evt"), dir.join("corpus.evt")).unwrap();
    let ddd = dir.join("clean.ddd");
    fs::write(&ddd, CLEAN_DDD).unwrap();
    (config, dir.join("corpus.mox"), dir.join("corpus.evt"), ddd)
}

async fn fixture_engine(config: &codegraph_config::DomainConfig) -> Backend {
    let backend = create_backend(&BackendConfig::default())
        .await
        .expect("in-memory backend");
    codegraph::ingest::mox_ingest::ingest_mox_files(
        backend.ingestor(),
        backend.querier(),
        &[fixture("corpus.mox")],
        config,
        &config.defaults.type_suffix,
    )
    .await
    .expect("mox ingest");
    codegraph::ingest::ddd_ingest::ingest_ddd_files(
        backend.ingestor(),
        backend.querier(),
        &[fixture("corpus.ddd")],
        config,
    )
    .await
    .expect("ddd ingest");
    codegraph::ingest::evt_ingest::ingest_evt_files(
        backend.ingestor(),
        backend.querier(),
        &[fixture("corpus.evt")],
        config,
    )
    .await
    .expect("evt ingest");
    backend
}

fn refusal_bodies(report: &codegraph_generate::events::producer::ProducerReport) -> Vec<&str> {
    report
        .refusals
        .iter()
        .map(|refusal| refusal.body.as_str())
        .collect()
}

/// The planner over the FULL corpus: every documented sentence, verbatim and
/// in declaration order, and no plan for any refused pair.
#[tokio::test]
async fn full_corpus_yields_every_documented_first_blocker_and_no_partial_plan() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = write_config(dir.path());
    let config = codegraph_config::config::parse_domain_config(&config_path).unwrap();
    let engine = fixture_engine(&config).await;

    let report = codegraph_generate::events::producer::plan_for_graph(
        engine.querier(),
        &config,
        false,
        DatabaseTarget::Postgres,
    )
    .await
    .expect("planner");

    assert_eq!(
        refusal_bodies(&report),
        DOCUMENTED_REFUSALS,
        "the corpus must yield exactly the documented first blockers, in declaration order"
    );

    // No partially bound payload: a refused pair never yields a plan.
    for (event, op) in REFUSED_PAIRS {
        assert!(
            !report
                .plans
                .iter()
                .any(|plan| plan.event.name == *event || plan.op == *op),
            "refused pair {event}/{op} must yield no plan; plans: {:?}",
            report
                .plans
                .iter()
                .map(|plan| (&plan.event.name, &plan.op))
                .collect::<Vec<_>>()
        );
    }

    // The clean shapes do plan — the refusal list above is the ONLY blocker.
    let planned: BTreeMap<&str, &str> = report
        .plans
        .iter()
        .map(|plan| (plan.event.name.as_str(), plan.op.as_str()))
        .collect();
    assert_eq!(
        planned.keys().copied().collect::<Vec<_>>(),
        vec!["JobStarted", "ShipmentBooked"],
        "the full corpus plans exactly its clean A/B pairs; got: {planned:?}"
    );
}

/// Flag-ON validation turns those same sentences into hard errors (the
/// reason the catalogue never has to render a refusal mark); flag-OFF
/// validation is silent over the same graph.
#[tokio::test]
async fn flag_on_validation_hard_fails_with_the_documented_sentences() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = write_config(dir.path());
    let config = codegraph_config::config::parse_domain_config(&config_path).unwrap();
    let engine = fixture_engine(&config).await;

    let issues = codegraph::validate::ValidationPass::run_with_options(
        engine.querier(),
        &config,
        codegraph::validate::ValidationOptions {
            workers_topology: false,
            events_producer: true,
            database_target: DatabaseTarget::Postgres,
        },
    )
    .await;
    let producer: Vec<&codegraph::validate::ValidationIssue> = issues
        .iter()
        .filter(|issue| {
            issue.check == "events_producer"
                && matches!(issue.severity, codegraph::validate::Severity::Error)
        })
        .collect();
    assert_eq!(
        producer.len(),
        DOCUMENTED_REFUSALS.len(),
        "one hard error per refused pair; got: {:?}",
        producer
            .iter()
            .map(|issue| &issue.message)
            .collect::<Vec<_>>()
    );
    for (issue, body) in producer.iter().zip(DOCUMENTED_REFUSALS) {
        assert!(
            matches!(issue.severity, codegraph::validate::Severity::Error),
            "every producer finding is a hard error: {issue:?}"
        );
        assert_eq!(
            issue.message,
            format!("events producer: {body}"),
            "the validation line is the documented sentence body"
        );
    }

    let off = codegraph::validate::ValidationPass::run(engine.querier(), &config).await;
    assert!(
        off.iter().all(|issue| issue.check != "events_producer"),
        "flag-off validation must stay silent"
    );
}

/// The flag-off pin's teeth: a plan-less run over the FULL corpus (which
/// carries every refusal) succeeds, because the flag gates the analysis.
#[tokio::test]
async fn flag_off_plan_less_run_succeeds_over_the_full_corpus() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = write_config(dir.path());
    let output = dir.path().join("out");

    codegraph::driver::run(codegraph::driver::RunArgs {
        schemas: None,
        classifier: None,
        config_path: &config_path,
        output: &output,
        extension_points_path: None,
        integration_manifests: &[],
        profile_name: "default",
        variant: None,
        // Nonexistent ⇒ the no-plan (all generators) backward-compat path.
        profiles_config_path: Some(PathBuf::from("definitely-absent-profiles.toml")),
        no_post_gen: true,
        template_dir: &[],
        ifml_files: &[],
        openapi_files: &[],
        mox_files: &[fixture("corpus.mox")],
        rosetta_files: &[],
        ddd_files: &[fixture("corpus.ddd")],
        evt_files: &[fixture("corpus.evt")],
        deploy_files: &[],
        ifml_framework: &[],
        ifml_components: None,
        ifml_design_system: None,
        codegraph_rev: None,
        check: false,
        ux_rules: None,
    })
    .await
    .expect("a flag-off run must not fail on producer refusals");
}

/// Generate the CLEAN subset under `events_producer` ON/OFF: the marks, the
/// publish seam, and the enumerated flag-off delta (the marks are the ONLY
/// difference in the catalogue).
#[tokio::test]
async fn clean_subset_publishes_and_the_catalogue_carries_the_exact_marks() {
    let dir = tempfile::tempdir().unwrap();
    let (config_path, mox, evt, ddd) = stage_clean_project(dir.path());
    let profiles_on = write_profiles(dir.path(), true);
    let profiles_off = write_profiles(dir.path(), false);

    let on = dir.path().join("out-on");
    run_generation(&config_path, &on, profiles_on, &mox, &evt, &ddd).await;
    let on_files = collect_files(&on);

    // The publishing seam: class A (patch) and class B (transition), fanning
    // out to every bound channel.
    let shipment = on_files
        .get("src/domain/corpus/shipment/command.rs")
        .unwrap_or_else(|| panic!("missing shipment command; files: {:?}", on_files.keys()));
    assert!(
        shipment.contains("pub async fn update_with_events("),
        "class A must publish through the transaction seam: {shipment}"
    );
    assert!(
        shipment.contains("crate::events::emit::publish_shipments_in_tx("),
        "class A publishes on the shipments channel: {shipment}"
    );
    assert!(
        shipment.contains("crate::events::emit::publish_audit_in_tx("),
        "the second bound channel is a fanout target (the `queues:` mark): {shipment}"
    );
    let job = on_files
        .get("src/domain/corpus/job/command.rs")
        .unwrap_or_else(|| panic!("missing job command; files: {:?}", on_files.keys()));
    assert!(
        job.contains("crate::events::emit::publish_jobs_in_tx("),
        "class B publishes on its transition: {job}"
    );
    assert!(
        job.contains("previous_state:"),
        "class B carries the transition source state: {job}"
    );

    // The catalogue marks — the drift pin between the planner and the
    // catalogue plane. Exact lines, exact queues.
    let catalogue = on_files
        .get("docs/events.md")
        .expect("the run must emit docs/events.md");
    for mark in [
        // The ops join in declaration order; the queues carry the plan's own
        // `publish` order (the planner's bound-channel order).
        "Runtime publication: published at runtime by ShipmentService.book, \
         ShipmentService.amend (arm: patch; queues: events_audit, events_shipments)",
        "Runtime publication: published at runtime by JobService.start \
         (arm: transition; queue: events_jobs)",
        "Runtime publication: published at runtime by JobService.touch, \
         JobService.retouch (arm: transition; queue: events_jobs)",
    ] {
        assert!(
            catalogue.contains(mark),
            "catalogue missing the mark `{mark}`:\n{catalogue}"
        );
    }
    let unbound = "Runtime publication: not emitted — no modeled service operation declares \
                   this event";
    assert_eq!(
        catalogue
            .matches("Runtime publication: published at runtime")
            .count(),
        3,
        "exactly the planned events carry a published mark:\n{catalogue}"
    );
    assert!(
        catalogue.matches(unbound).count() >= 1,
        "a declared event no operation binds carries the unbound mark:\n{catalogue}"
    );
    // The unbound line sits under its own event, directly after the table.
    let recorded = catalogue
        .split("## `ShipmentStatusRecorded`")
        .nth(1)
        .and_then(|tail| tail.split("### Payload").next())
        .expect("the ShipmentStatusRecorded section");
    assert!(
        recorded.contains(unbound),
        "the unbound mark must sit inside the declared event's section:\n{recorded}"
    );

    // Flag OFF over the same subset: no marks at all, and the flag-ON
    // catalogue is the flag-OFF document plus exactly those lines.
    let off = dir.path().join("out-off");
    run_generation(&config_path, &off, profiles_off, &mox, &evt, &ddd).await;
    let off_files = collect_files(&off);
    let off_catalogue = off_files
        .get("docs/events.md")
        .expect("the flag-off run emits the catalogue too");
    assert!(
        !off_catalogue.contains("Runtime publication:"),
        "flag-off output must carry no marks:\n{off_catalogue}"
    );
    assert_eq!(
        strip_marks(catalogue),
        *off_catalogue,
        "the flags differ by the mark lines ONLY (byte-identity for the rest)"
    );
    let off_shipment = off_files
        .get("src/domain/corpus/shipment/command.rs")
        .expect("shipment command");
    assert!(
        !off_shipment.contains("update_with_events("),
        "flag-off emits the plain command: {off_shipment}"
    );
}

/// Drop every mark line (and the blank line it introduced) — the enumerated
/// flag-ON → flag-OFF delta of the catalogue.
fn strip_marks(content: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let mut skip_blank = false;
    for line in content.lines() {
        if line.starts_with("Runtime publication:") {
            skip_blank = true;
            continue;
        }
        if skip_blank && line.is_empty() {
            skip_blank = false;
            continue;
        }
        skip_blank = false;
        out.push(line);
    }
    let mut joined = out.join("\n");
    joined.push('\n');
    joined
}

#[allow(clippy::too_many_arguments)]
async fn run_generation(
    config_path: &Path,
    output: &Path,
    profiles: PathBuf,
    mox: &Path,
    evt: &Path,
    ddd: &Path,
) {
    codegraph::driver::run(codegraph::driver::RunArgs {
        schemas: None,
        classifier: None,
        config_path,
        output,
        extension_points_path: None,
        integration_manifests: &[],
        profile_name: "default",
        variant: None,
        profiles_config_path: Some(profiles),
        no_post_gen: true,
        template_dir: &[],
        ifml_files: &[],
        openapi_files: &[],
        mox_files: &[mox.to_path_buf()],
        rosetta_files: &[],
        ddd_files: &[ddd.to_path_buf()],
        evt_files: &[evt.to_path_buf()],
        deploy_files: &[],
        ifml_framework: &[],
        ifml_components: None,
        ifml_design_system: None,
        codegraph_rev: None,
        check: false,
        ux_rules: None,
    })
    .await
    .expect("generation over the clean corpus subset");
}

fn collect_files(root: &Path) -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    collect_into(root, root, &mut files);
    files
}

fn collect_into(root: &Path, dir: &Path, files: &mut BTreeMap<String, String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|entry| entry.ok()) {
        let path = entry.path();
        if path.is_dir() {
            collect_into(root, &path, files);
        } else if let (Ok(rel), Ok(content)) = (
            path.strip_prefix(root)
                .map(|path| path.to_string_lossy().to_string()),
            fs::read_to_string(&path),
        ) {
            files.insert(rel.replace('\\', "/"), content);
        }
    }
}

/// The product default: the shipped `profiles.toml` keeps the producer flag
/// OFF, so a consumer's generated tree carries neither the publishing seam nor
/// the catalogue marks. The repo's own canaries build their in-test profiles,
/// so this is the pin that holds the SHIPPED default (issue #820 slice c's
/// byte-identity premise: this slice must never flip it).
#[test]
fn shipped_profiles_keep_the_producer_flag_off() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let profiles = fs::read_to_string(workspace.join("profiles.toml")).expect("workspace profiles");
    for feature in profiles
        .lines()
        .filter(|line| line.trim_start().starts_with("events_producer"))
    {
        assert_eq!(
            feature.trim(),
            "events_producer = false",
            "the shipped default must keep events_producer OFF"
        );
    }
    assert!(
        profiles.contains("events_producer = false"),
        "the shipped profiles must declare events_producer = false explicitly"
    );
}
