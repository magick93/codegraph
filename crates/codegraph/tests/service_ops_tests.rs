//! Service operations lower to API surface (issue #590).
//!
//! A public `.ddd` service op with a resolvable entity binding emits
//! `POST /api/{version}/{domain}/{entity}/{id}/actions/{op_snake}` — the
//! wire shape the IFML action buttons already POST (#559) — with the actor
//! policy enforced server-side: capability membership, `never_both`
//! separation-of-duties conflicts, and per-grant `when` conditions all
//! evaluate in the handler over the loaded record.
//!
//! Pins here (node-free PR-CI):
//! - routes + handlers + authorize fns (capability / SoD / `when`) render
//! - wired delegation legs (delete → command, findById → read) and the
//!   501 seam naming the gap for everything else
//! - `dead_surface_warnings` + the run-path validation WARNs (Ask 1, Ask 3)
//! - no `.ddd` services ⇒ byte-identical router/handler (presence gating)

#[path = "service_ops_fixture.rs"]
mod fixture;

use std::fs;
use std::path::{Path, PathBuf};

use codegraph_core::traits::GraphQuerier;
use tempfile::TempDir;

use fixture::{
    DOMAINS_TOML, Fixture, PAY_RUN_MOX, PAY_RUN_SCHEMA, PAYROLL_DDD, ingest, read, run_generation,
    tempdir, write_fixture_in,
};

/// The first `HTTP <status>` after `needle` in a hurl file.
fn status_after(hay: &str, needle: &str) -> Option<u16> {
    let pos = hay.find(needle)?;
    let rest = &hay[pos..];
    let http = rest.find("HTTP ")?;
    rest[http + 5..].split_whitespace().next()?.parse().ok()
}

/// The last `HTTP <status>` after `needle` (the replay leg).
fn status_after_last(hay: &str, needle: &str) -> Option<u16> {
    let pos = hay.rfind(needle)?;
    let rest = &hay[pos..];
    let http = rest.find("HTTP ")?;
    rest[http + 5..].split_whitespace().next()?.parse().ok()
}

// ── Pins ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn service_ops_render_routes_handlers_and_policy_gates() {
    let fixture = write_fixture_in("service_ops_fixture", PAYROLL_DDD);
    let engine = ingest(&fixture).await;
    let out = tempdir();
    let domain_types = tempdir();
    let hooks = tempdir();
    let report = run_generation(
        &engine,
        &fixture,
        out.path(),
        domain_types.path(),
        hooks.path(),
    )
    .await;
    assert!(!report.has_errors(), "generation errors: {report:?}");

    // The `.ddd` machine survives ingest → graph → readback (P0.5a) and
    // rehydrates for condition/transition lowering.
    let models = engine.get_ddd_models().await.unwrap();
    let design = models
        .iter()
        .flat_map(|m| &m.designs)
        .find(|d| d.resolved_title.as_deref() == Some("PayRunType"))
        .expect("PayRunType design");
    assert!(design.machine_json.is_some(), "machine must be mirrored");
    let surface = codegraph_generate::ddd::design::DddDesignSurface::from_graph(&engine)
        .await
        .unwrap();
    let mut states = surface.machine_states_for("PayRunType");
    states.sort();
    assert_eq!(states, vec!["Approved", "Calculating", "Draft", "Reversed"]);

    // A mox enum in a non-`common` package still emits its GLOBAL codelist
    // table into `common` (FK resolution routes every codelist there) — the
    // `relation "common.<table>" does not exist` regression.
    let codelist_migration = out
        .path()
        .join("migrations/000010_common_pay_run_status_codelist.sql");
    let codelist = read(&codelist_migration);
    assert!(
        codelist.contains("CREATE TABLE IF NOT EXISTS common.pay_run_status"),
        "the codelist table must land in common:\n{codelist}"
    );
    assert!(
        codelist.contains("'Draft'"),
        "the enum values must seed:\n{codelist}"
    );

    // ── router: the four action routes on the root entity ──
    let router = read(&out.path().join("src/api/payroll/router.rs"));
    for snake in ["approve", "inspect", "archive", "submit"] {
        assert!(
            router.contains(&format!("/actions/{snake}\"")),
            "router must register actions/{snake}:\n{router}"
        );
    }
    assert!(
        router.contains("pay_run_handler::service_op_approve"),
        "the route resolves into the handler module:\n{router}"
    );

    // ── handler: authorize fn + policy gates + execution legs ──
    let handler = read(&out.path().join("src/api/payroll/pay_run_handler.rs"));
    // api_key bypass: contract suites and service callers keep working.
    assert!(handler.contains(r#"if role == "api_key""#), "{handler}");
    // Owner arm: unconditional permit → no guard.
    assert!(handler.contains(r#""owner" => {"#), "{handler}");
    // Owner arm on `approve`: the MERGED policy has Owner holding both
    // sides of the `never_both` pair — the runtime SoD gate denies.
    assert!(
        handler.contains("separation of duties: role `owner` holds `PreparePayRun`"),
        "the never_both conflict must deny owner:\n{handler}"
    );
    // Manager arm on `approve`: holds nothing relevant → falls to deny.
    assert!(
        handler.contains("capability `ApprovePayRun` required"),
        "{handler}"
    );
    // The op-level machine-state `when` binds the declared state (P0.5) and
    // applies to every session — before the api_key bypass. Scope to the
    // submit authorize fn (earlier routes carry their own bypass).
    let submit_start = handler
        .find("fn service_op_submit_authorize")
        .expect("submit authorize fn");
    let submit_fn = &handler[submit_start..];
    let condition_pos = submit_fn
        .find(r#"// op-level `when` transition guard"#)
        .expect("op-level guard");
    let api_key_pos = submit_fn
        .find(r#"if role == "api_key""#)
        .expect("api_key bypass");
    assert!(
        condition_pos < api_key_pos,
        "the transition guard must precede the API-key bypass:\n{submit_fn}"
    );
    assert!(
        submit_fn.contains(r#"if !((response.pay_run_status == "Draft"))"#),
        "the machine state must lower as the enum value:\n{submit_fn}"
    );
    // Capability-gated route answers 403 naming the reason.
    assert!(handler.contains("AppError::forbidden(reason)"), "{handler}");
    // Wired deletion leg: approve delegates to the repository delete
    // builtin → the command plane.
    assert!(
        handler.contains("_commands.delete(pay_run_id,"),
        "the delete delegation must lower to the command plane:\n{handler}"
    );
    // Wired read leg.
    assert!(handler.contains(r#""data": response"#), "{handler}");
    // Executable save/update delegation (P0): `archive` and `submit` patch
    // the loaded row through the update command and return the reloaded row.
    assert!(
        handler.contains("_commands.update("),
        "the save delegation must lower to the update command:\n{handler}"
    );
    assert!(
        handler.contains(r#""data": updated"#),
        "the update leg must return the reloaded record:\n{handler}"
    );
    // `submit` binds the machine transition and patches the state field.
    assert!(
        handler.contains(r#""Draft" => Some("Calculating")"#),
        "the machine transition must resolve from the current state:\n{handler}"
    );
    assert!(
        handler.contains(r#"value["pay_run_status"] = serde_json::Value::String"#),
        "the transition must patch the DTO state key:\n{handler}"
    );
    // The abstract op still seams.
    assert!(handler.contains("NOT_IMPLEMENTED"), "{handler}");
    assert!(
        handler.contains("has no delegation target"),
        "the abstract-op seam must name the gap:\n{handler}"
    );
    // The protected op emits no route.
    assert!(
        !handler.contains("internal_adjust"),
        "protected ops stay off the API plane:\n{handler}"
    );
    // OpenAPI registers the action paths.
    let openapi = read(&out.path().join("src/api/openapi/payroll.rs"));
    assert!(
        openapi.contains("service_op_approve"),
        "openapi must register the action paths:\n{openapi}"
    );

    // ── hurl: per-entity actions contract ──
    // Numbering mirrors the entity contract's global order; this fixture
    // has one entity so the file is 10_payroll_pay_run_actions.hurl.
    let actions = read(&out.path().join("hurl/10_payroll_pay_run_actions.hurl"));
    // A fresh fixture is created and captured before the action legs.
    assert!(actions.contains("HTTP 201"), "{actions}");
    assert!(actions.contains("/actions/approve"), "{actions}");
    // Wired delegation legs pin 200; wrong-state transitions pin 409 (the
    // machine is simulated from its initial state); the abstract op still
    // pins 501.
    assert!(actions.contains("HTTP 200"), "{actions}");
    assert!(actions.contains("HTTP 409"), "{actions}");
    assert!(actions.contains("HTTP 501"), "{actions}");
    assert!(
        actions.contains("wrong-state guard (409)") || actions.contains("wrong-state replay"),
        "the contract must document the wrong-state leg:\n{actions}"
    );
    // The machine is simulated from Draft: `reverse` (Approved → Reversed)
    // is out of order and pins 409; the linear lifecycle pins 200; the
    // abstract op pins 501; the final replay proves the wrong-state edge.
    assert_eq!(
        status_after(&actions, "/actions/reverse"),
        Some(409),
        "{actions}"
    );
    assert_eq!(
        status_after(&actions, "/actions/submit"),
        Some(200),
        "{actions}"
    );
    assert_eq!(
        status_after(&actions, "/actions/approve"),
        Some(200),
        "{actions}"
    );
    assert_eq!(
        status_after(&actions, "/actions/notify"),
        Some(501),
        "{actions}"
    );
    assert_eq!(
        status_after_last(&actions, "/actions/approve"),
        Some(409),
        "the wrong-state replay must pin 409:\n{actions}"
    );
    assert!(
        !actions.contains("delegates to repository `save`, which has"),
        "the save delegation must no longer seam:\n{actions}"
    );
    assert!(
        actions.contains("has no delegation target"),
        "the abstract-op seam must name the gap:\n{actions}"
    );
    // Policy note: the 403 surface rides JWT sessions; api keys bypass.
    assert!(actions.contains("bypass the policy gates"), "{actions}");
}

/// P2 determinism: a mox enum outside `common` emits its GLOBAL codelist
/// table into `common`, and two runs produce the same migration number and
/// bytes (the codelist order is sorted at the allocation boundary).
#[tokio::test]
async fn non_common_mox_codelists_are_common_and_deterministic() {
    let fixture = write_fixture_in("service_ops_codelist_determinism", PAYROLL_DDD);
    let engine = ingest(&fixture).await;
    let mut rendered = Vec::new();
    for _ in 0..2 {
        let out = tempdir();
        let domain_types = tempdir();
        let hooks = tempdir();
        let report = run_generation(
            &engine,
            &fixture,
            out.path(),
            domain_types.path(),
            hooks.path(),
        )
        .await;
        assert!(!report.has_errors(), "generation errors: {report:?}");
        rendered.push(read(
            &out.path()
                .join("migrations/000010_common_pay_run_status_codelist.sql"),
        ));
    }
    assert_eq!(
        rendered[0], rendered[1],
        "codelist output must be deterministic"
    );
    assert!(
        rendered[0].contains("common.pay_run_status"),
        "{}",
        rendered[0]
    );
}

// The refusal discipline (un-lowerable `when` conditions hard-fail
// generation naming the op) is pinned at the unit level
// (`refusals_name_the_capability` in `ddd::service_ops`): the upstream
// rex-driver typechecker already rejects most of the out-of-subset
// surface before the graph, so the graph-side refusal is defense-in-depth
// for `.json` actor artifacts.

#[tokio::test]
async fn dead_surface_warnings_report_protected_and_unbound_ops() {
    let fixture = write_fixture_in("service_ops_dead_surface", PAYROLL_DDD);
    let engine = ingest(&fixture).await;
    let config = codegraph_config::config::parse_domain_config(&fixture.config).unwrap();
    let warnings = codegraph_generate::ddd::service_ops::dead_surface_warnings(&engine, &config)
        .await
        .unwrap();
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("PayRunService::internalAdjust") && w.contains("protected")),
        "protected ops must warn: {warnings:?}"
    );
}

#[tokio::test]
async fn validation_pass_warns_on_dead_surface_and_status_bypass() {
    let fixture = write_fixture_in("service_ops_validation", PAYROLL_DDD);
    let engine = ingest(&fixture).await;
    let config = codegraph_config::config::parse_domain_config(&fixture.config).unwrap();
    let issues = codegraph::validate::ValidationPass::run(&engine, &config).await;
    // Ask 1: the protected op is dead surface.
    assert!(
        issues.iter().any(|i| {
            matches!(i.severity, codegraph::validate::Severity::Warning)
                && i.check == "service_op_dead_surface"
                && i.message.contains("PayRunService::internalAdjust")
        }),
        "the protected op must warn: {issues:?}"
    );
    // Ask 3: a workflow whose status field is a bare String is bypassable.
    let workflow_config = fixture.config.clone();
    let mut text = fs::read_to_string(&workflow_config).unwrap();
    text.push_str(
        "\n[domains.payroll.entity_config.PayRunType.workflow]\n\
         status_field = \"pay_run_status\"\n\
         states = [\"Draft\", \"AwaitingApproval\", \"Posted\"]\n\
         initial_state = \"Draft\"\n\
         terminal_states = [\"Posted\"]\n",
    );
    fs::write(&workflow_config, text).unwrap();
    let config = codegraph_config::config::parse_domain_config(&workflow_config).unwrap();
    let issues = codegraph::validate::ValidationPass::run(&engine, &config).await;
    assert!(
        issues.iter().any(|i| {
            matches!(i.severity, codegraph::validate::Severity::Warning)
                && i.check == "status_bypass"
                && i.message.contains("bypassable via scaffold CRUD")
        }),
        "the bare-String status field must warn: {issues:?}"
    );
}

#[tokio::test]
async fn no_ddd_services_keeps_router_and_handler_free_of_actions() {
    // Schemas + mox only: no `.ddd` input ⇒ no service nodes ⇒ no action
    // routes (presence gating; the standing canaries pin full byte-identity).
    let dir = TempDir::new().unwrap();
    // SchemaLoader layout: <schema_dir>/<domain>/json/*.json.
    let json_dir = dir
        .path()
        .join("model-schemas")
        .join("payroll")
        .join("json");
    fs::create_dir_all(&json_dir).unwrap();
    fs::write(json_dir.join("PayRunType.json"), PAY_RUN_SCHEMA).unwrap();
    let schemas_dir = dir.path().join("model-schemas");
    let mox = dir.path().join("pay_run.mox");
    fs::write(&mox, PAY_RUN_MOX).unwrap();
    let config_path = dir.path().join("domains.toml");
    fs::write(&config_path, DOMAINS_TOML).unwrap();
    std::mem::forget(dir);

    let engine = codegraph_grafeo::GrafeoEngine::in_memory().unwrap();
    let config = codegraph_config::config::parse_domain_config(&config_path).unwrap();
    let classifier = codegraph_classifier::config::parse_classifier_config(Path::new(
        "tests/fixtures/classifier.toml",
    ))
    .unwrap();
    let entity_names: std::collections::HashSet<String> = config
        .domains
        .values()
        .flat_map(|d| d.entities.iter().cloned())
        .collect();
    codegraph::ingest::async_ingest::ingest_schemas(
        &engine,
        &schemas_dir,
        &classifier,
        &entity_names,
        &codegraph_config::UiOverrideConfig::default(),
        &config.defaults.type_suffix,
    )
    .await
    .unwrap();
    codegraph::ingest::mox_ingest::ingest_mox_files(
        &engine,
        &engine,
        &[mox],
        &config,
        &config.defaults.type_suffix,
    )
    .await
    .unwrap();

    let fixture = Fixture {
        schemas_dir,
        mox: PathBuf::new(),
        ddd: PathBuf::new(),
        actor: PathBuf::new(),
        config: config_path,
    };
    let out = tempdir();
    let domain_types = tempdir();
    let hooks = tempdir();
    let report = run_generation(
        &engine,
        &fixture,
        out.path(),
        domain_types.path(),
        hooks.path(),
    )
    .await;
    assert!(!report.has_errors(), "generation errors: {report:?}");

    let router = read(&out.path().join("src/api/payroll/router.rs"));
    assert!(
        !router.contains("/actions/"),
        "no services ⇒ no action routes:\n{router}"
    );
    let handler = read(&out.path().join("src/api/payroll/pay_run_handler.rs"));
    assert!(
        !handler.contains("service_op_"),
        "no services ⇒ no op handlers:\n{handler}"
    );
}
