//! Events-producer planner end-to-end (issue #820 slice a).
//!
//! Node-free: the fixture `.mox`/`.ddd`/`.evt` sources are ingested into a
//! `MockEngine`, then the SAME planner the driver runs is asserted over the
//! documented sentences. Also pins the flag gate (ON ⇒ hard validation
//! errors; OFF ⇒ zero `events_producer` diagnostics and a successful
//! plan-less run), the doctor diagnostic surface (same sentence body, WARN,
//! exit 0), and the flag-inert generation contract (two profiles differing
//! only in `events_producer` produce a byte-identical tree).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use codegraph::init::commands::{DoctorArgs, cmd_doctor};
use codegraph_backend::{Backend, BackendConfig, create_backend};
use codegraph_generate::db::dialect::DatabaseTarget;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/events_producer")
        .join(name)
}

/// The fixture's bounded context. `OrderType.status` is workflow-managed, so
/// the Update DTO excludes it — the R6 `…Changed` anchor the planner refuses.
const DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.orders]
label = "Orders"
schema_dir = "orders"
postgres_schema = "orders"

[domains.orders.entity_config.OrderType.workflow]
status_field = "status"
states = ["Draft", "Placed", "Cancelled"]
initial_state = "Draft"
terminal_states = ["Cancelled"]
"#;

/// The minimal profile the flag-inertness pin uses. `events_producer` is the
/// ONLY difference between the two runs.
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

fn write_config(dir: &Path) -> PathBuf {
    let path = dir.join("domains.toml");
    fs::write(&path, DOMAINS_TOML).unwrap();
    path
}

async fn fixture_engine(config: &codegraph_config::DomainConfig) -> Backend {
    let backend = create_backend(&BackendConfig::default())
        .await
        .expect("in-memory backend");
    codegraph::ingest::mox_ingest::ingest_mox_files(
        backend.ingestor(),
        backend.querier(),
        &[fixture("orders.mox")],
        config,
        &config.defaults.type_suffix,
    )
    .await
    .expect("mox ingest");
    codegraph::ingest::ddd_ingest::ingest_ddd_files(
        backend.ingestor(),
        backend.querier(),
        &[fixture("orders.ddd")],
        config,
    )
    .await
    .expect("ddd ingest");
    codegraph::ingest::evt_ingest::ingest_evt_files(
        backend.ingestor(),
        backend.querier(),
        &[fixture("orders.evt")],
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

#[tokio::test]
async fn the_planner_produces_the_documented_sentences_end_to_end() {
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

    // The one clean pair: OrderPlaced/place publishes record-compatible
    // fields; no W2 (a plain patch can write both fields through its Update
    // DTO).
    assert!(
        report
            .plans
            .iter()
            .any(|plan| plan.event.name == "OrderPlaced" && plan.op == "place"),
        "expected the clean OrderPlaced/place plan; plans: {:?}",
        report
            .plans
            .iter()
            .map(|p| (&p.event.name, &p.op))
            .collect::<Vec<_>>()
    );
    assert!(
        report.warnings.is_empty(),
        "unexpected warnings: {:?}",
        report.warnings
    );

    let bodies = refusal_bodies(&report);

    // R1 — the `_code`-stripped near miss.
    assert!(
        bodies.contains(
            &"event `OrderStatusCoded` (operation `OrderService.recordStatusCode`) payload field \
              `statusCode` has no matching field on `OrderResponse`; nearest: `status` — \
              rename the payload field in the .evt contract or the class feature so the \
              spellings agree"
        ),
        "missing R1 `_code` refusal; got: {bodies:#?}"
    );

    // R1 — nothing within edit distance 2.
    assert!(
        bodies.iter().any(|body| body.contains("`xyzzy`")
            && body.contains("nearest: (none within edit distance 2)")),
        "missing R1 none clause; got: {bodies:#?}"
    );

    // R2 — optional record source.
    assert!(
        bodies.iter().any(|body| body.contains("`note` is required")
            && body.contains("`OrderResponse.note` is nullable")),
        "missing R2 refusal; got: {bodies:#?}"
    );

    // R3 — contained child collection.
    assert!(
        bodies.iter().any(|body| body.contains("`quantity`")
            && body.contains("contained collection `Order.lines`")),
        "missing R3 refusal; got: {bodies:#?}"
    );

    // R5 — duplicate emission (both occurrences refused, one line).
    assert!(
        bodies.contains(
            &"event `OrderPlaced` is bound to operation `OrderService.duplicate` more than once \
              and the conditions can both hold — split the operations or make the conditions \
              exclusive"
        ),
        "missing R5 refusal; got: {bodies:#?}"
    );
    assert!(
        !report.plans.iter().any(|plan| plan.op == "duplicate"),
        "a duplicate emission must not yield a plan"
    );

    // R6 — a `…Changed` fact on a plain patch that cannot write the anchor.
    assert!(
        bodies.contains(
            &"event `OrderStatusChanged` is a change fact bound to plain patch operation \
              `OrderService.changeStatus` which does not write `status` — bind the emission to \
              the operation that owns the fact"
        ),
        "missing R6 refusal; got: {bodies:#?}"
    );

    // R9 — the op binds no entity at all (a declared op with no delegated
    // target, capability class, or class-typed signature).
    assert!(
        bodies
            .iter()
            .any(|body| body.contains("`OrderService.orphan`")
                && body.contains("has no model source")),
        "missing R9 unbound-op refusal; got: {bodies:#?}"
    );

    // R4 — protected / abstract / read-only runtime sites.
    assert!(
        bodies
            .iter()
            .any(|body| body.contains("`OrderService.archive`") && body.contains("(protected)")),
        "missing R4 protected; got: {bodies:#?}"
    );
    assert!(
        bodies
            .iter()
            .any(|body| body.contains("`OrderService.notify`") && body.contains("(abstract)")),
        "missing R4 abstract; got: {bodies:#?}"
    );
    assert!(
        bodies
            .iter()
            .any(|body| body.contains("`OrderService.find`") && body.contains("(read-only)")),
        "missing R4 read-only; got: {bodies:#?}"
    );
}

#[tokio::test]
async fn flag_off_contributes_zero_events_producer_diagnostics() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = write_config(dir.path());
    let config = codegraph_config::config::parse_domain_config(&config_path).unwrap();
    let engine = fixture_engine(&config).await;

    let issues = codegraph::validate::ValidationPass::run(engine.querier(), &config).await;
    assert!(
        issues.iter().all(|issue| issue.check != "events_producer"),
        "flag-off validation must be silent; got: {:?}",
        issues
            .iter()
            .filter(|issue| issue.check == "events_producer")
            .map(|issue| &issue.message)
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn flag_on_turns_the_refusals_into_hard_errors() {
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
            ..Default::default()
        },
    )
    .await;
    let producer: Vec<_> = issues
        .iter()
        .filter(|issue| issue.check == "events_producer")
        .collect();
    assert!(
        !producer.is_empty(),
        "flag-on validation must report the producer refusals"
    );
    assert!(
        producer.iter().any(
            |issue| matches!(issue.severity, codegraph::validate::Severity::Error)
                && issue
                    .message
                    .starts_with("events producer: event `OrderStatusCoded`")
        ),
        "expected the R1 hard error; got: {:?}",
        producer
            .iter()
            .map(|issue| &issue.message)
            .collect::<Vec<_>>()
    );
}

/// The flag-off pin's teeth: a plan-less run over a model carrying producer
/// refusals SUCCEEDS. If the validator ignored the flag, this would fail.
#[tokio::test]
async fn flag_off_plan_less_run_succeeds_over_a_model_with_refusals() {
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
        mox_files: &[fixture("orders.mox")],
        rosetta_files: &[],
        ddd_files: &[fixture("orders.ddd")],
        evt_files: &[fixture("orders.evt")],
        deploy_files: &[],
        ifml_framework: &[],
        ifml_components: None,
        ifml_design_system: None,
        codegraph_rev: None,
        check: false,
        ux_rules: None,
    })
    .await
    .expect("flag-off run must not fail on producer refusals");
}

#[tokio::test]
async fn doctor_prints_the_same_lines_as_warn_and_exits_zero() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = write_config(dir.path());
    let config = codegraph_config::config::parse_domain_config(&config_path).unwrap();
    let profiles = dir.path().join("profiles.toml");
    fs::write(&profiles, PROFILE_TEMPLATE.replace("{flag}", "false")).unwrap();

    let args = doctor_args(&config_path, &profiles);
    let lines =
        codegraph::doctor_events::producer_diagnostics(&args, &config, DatabaseTarget::Postgres);
    assert!(
        lines.iter().any(|line| line
            == "WARN events producer — event `OrderStatusCoded` (operation \
                `OrderService.recordStatusCode`) payload field `statusCode` has no matching \
                field on `OrderResponse`; nearest: `status` — rename the payload field in \
                the .evt contract or the class feature so the spellings agree"),
        "doctor must print the same body as WARN; got: {lines:#?}"
    );
    assert!(
        lines
            .iter()
            .all(|line| line.starts_with("WARN events producer —")),
        "every producer line is a WARN here; got: {lines:#?}"
    );

    let summary = cmd_doctor(&args).expect("doctor must exit 0");
    assert_eq!(summary.hard_failures, 0, "doctor must not hard-fail");
}

#[tokio::test]
async fn flag_off_output_has_no_publishing_surface() {
    let dir = tempfile::tempdir().unwrap();
    let (config_path, mox, evt, ddd) = write_clean_project(dir.path());
    let profiles = dir.path().join("profiles-off.toml");
    fs::write(&profiles, PROFILE_TEMPLATE.replace("{flag}", "false")).unwrap();
    let output = dir.path().join("out-off");
    codegraph::driver::run(codegraph::driver::RunArgs {
        schemas: None,
        classifier: None,
        config_path: &config_path,
        output: &output,
        extension_points_path: None,
        integration_manifests: &[],
        profile_name: "default",
        variant: None,
        profiles_config_path: Some(profiles),
        no_post_gen: true,
        template_dir: &[],
        ifml_files: &[],
        openapi_files: &[],
        mox_files: std::slice::from_ref(&mox),
        rosetta_files: &[],
        ddd_files: std::slice::from_ref(&ddd),
        evt_files: std::slice::from_ref(&evt),
        deploy_files: &[],
        ifml_framework: &[],
        ifml_components: None,
        ifml_design_system: None,
        codegraph_rev: None,
        check: false,
        ux_rules: None,
    })
    .await
    .expect("flag-off generation");

    let tree = collect_files(&output);
    let all = tree.values().cloned().collect::<String>();
    for symbol in [
        "update_with_events",
        "OrderOutgoingEvent",
        "publish_orders_in_tx",
        "extract_traceparent",
        "entity_table: Some(",
    ] {
        assert!(
            !all.contains(symbol),
            "flag-off output must not carry `{symbol}`"
        );
    }
    // The flag-off bytes are held to master by the standing canaries
    // (`evt_pipeline_tests`, `evt_workers_tests`, `review_fixture_drift`).
}

#[tokio::test]
async fn flag_on_generates_the_publishing_command_and_handler() {
    let dir = tempfile::tempdir().unwrap();
    let (config_path, mox, evt, ddd) = write_clean_project(dir.path());
    let profiles = dir.path().join("profiles.toml");
    fs::write(&profiles, PROFILE_TEMPLATE.replace("{flag}", "true")).unwrap();
    let output = dir.path().join("out-on");
    codegraph::driver::run(codegraph::driver::RunArgs {
        schemas: None,
        classifier: None,
        config_path: &config_path,
        output: &output,
        extension_points_path: None,
        integration_manifests: &[],
        profile_name: "default",
        variant: None,
        profiles_config_path: Some(profiles),
        no_post_gen: true,
        template_dir: &[],
        ifml_files: &[],
        openapi_files: &[],
        mox_files: std::slice::from_ref(&mox),
        rosetta_files: &[],
        ddd_files: std::slice::from_ref(&ddd),
        evt_files: std::slice::from_ref(&evt),
        deploy_files: &[],
        ifml_framework: &[],
        ifml_components: None,
        ifml_design_system: None,
        codegraph_rev: None,
        check: false,
        ux_rules: None,
    })
    .await
    .expect("flag-on generation");

    let command = fs::read_to_string(output.join("src/domain/orders/order/command.rs")).unwrap();
    assert!(
        command.contains("pub enum OrderOutgoingEvent {"),
        "missing enum: {command}"
    );
    assert!(
        command.contains("pub async fn update_with_events("),
        "missing seam: {command}"
    );
    assert!(
        command.contains("crate::events::emit::publish_orders_in_tx("),
        "missing publish: {command}"
    );
    assert!(
        command.contains("entity_table: Some(\"order\")"),
        "missing envelope identity: {command}"
    );
    assert!(
        command.contains("let after = self"),
        "missing post-write re-read: {command}"
    );
    assert!(
        command.contains("order_number: after.order_number.clone()"),
        "missing typed construction: {command}"
    );

    let handler = fs::read_to_string(output.join("src/api/orders/order_handler.rs")).unwrap();
    assert!(
        handler.contains("update_with_events("),
        "handler must call the publishing seam: {handler}"
    );
    assert!(
        handler
            .contains("&[crate::domain::orders::order::command::OrderOutgoingEvent::OrderPlaced]"),
        "missing outgoing variants: {handler}"
    );
    assert!(
        handler.contains("extract_traceparent(&headers)"),
        "missing traceparent capture: {handler}"
    );
    assert!(
        handler.contains("fn extract_traceparent("),
        "missing traceparent helper: {handler}"
    );

    let emit = fs::read_to_string(output.join("src/events/emit.rs")).unwrap();
    assert!(emit.contains("pub async fn publish_orders_in_tx("));
    assert!(emit.contains("db: &impl ConnectionTrait"));
    assert!(emit.contains("pub entity_table: Option<&'static str>"));
}

/// A CLEAN model (only the matching `place` op emits) shared by the
/// flag-inert and flag-on generation pins.
fn write_clean_project(dir: &Path) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let config_path = write_config(dir);
    fs::copy(fixture("orders.mox"), dir.join("orders.mox")).unwrap();
    fs::copy(fixture("orders.evt"), dir.join("orders.evt")).unwrap();
    fs::write(
        dir.join("clean.ddd"),
        r#"import "orders.mox";

application Orders {
    base nz.example.orders

    module orders {
        entity OrderType scaffold repository OrderRepository {
            findById;
            findAll;
            save;
            delete;
        }
        service OrderService {
            inject OrderRepository;
            place => OrderRepository.save emits OrderPlaced;
        }
    }
}
"#,
    )
    .unwrap();
    (
        config_path,
        dir.join("orders.mox"),
        dir.join("orders.evt"),
        dir.join("clean.ddd"),
    )
}

fn doctor_args(config: &Path, profiles: &Path) -> DoctorArgs {
    DoctorArgs {
        config: config.to_path_buf(),
        schemas: None,
        classifier: None,
        profiles_config: Some(profiles.to_path_buf()),
        mox_files: vec![fixture("orders.mox")],
        rosetta_files: Vec::new(),
        ddd_files: vec![fixture("orders.ddd")],
        evt_files: vec![fixture("orders.evt")],
        deploy_files: Vec::new(),
        ifml_files: Vec::new(),
        extension_points: None,
        fail_on_findings: false,
        graph_artifact: None,
        emit_graph: None,
        graph_reachability: Vec::new(),
    }
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
