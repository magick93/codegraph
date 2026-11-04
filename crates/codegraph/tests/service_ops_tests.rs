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

use tempfile::TempDir;

use fixture::{
    DOMAINS_TOML, Fixture, PAY_RUN_MOX, PAY_RUN_SCHEMA, PAYROLL_DDD, ingest, read, run_generation,
    tempdir, write_fixture_in,
};

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
    // Manager arm on `inspect`: conditional permit → when guard.
    assert!(
        handler.contains(r#"if !((response.status == "Draft"))"#),
        "the per-grant conditional `when` must render:\n{handler}"
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
    // Seam legs name the gap.
    assert!(handler.contains("NOT_IMPLEMENTED"), "{handler}");
    assert!(
        handler.contains("delegates to repository `save`, which has"),
        "the save-delegation seam must name the gap:\n{handler}"
    );
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
    // Wired delegation legs pin 200; seams pin 501 naming the gap.
    assert!(actions.contains("HTTP 501"), "{actions}");
    assert!(
        actions.contains("delegates to repository `save`, which has"),
        "{actions}"
    );
    // Policy note: the 403 surface rides JWT sessions; api keys bypass.
    assert!(actions.contains("bypass the policy gates"), "{actions}");
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
         status_field = \"status\"\n\
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
