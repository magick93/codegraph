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
    tempdir, write_fixture_in, write_mox_first_fixture_in,
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
    // Numbering mirrors the entity contract's global order; the fixture's
    // title-sort puts PayRunLineType first, so pay_run's actions file is
    // 11_payroll_pay_run_actions.hurl.
    let actions = read(&out.path().join("hurl/11_payroll_pay_run_actions.hurl"));
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

/// WS4: a service op bound to a refers-linked CHILD entity (`role = "child"`
/// with parent/parent_ref) emits a NESTED action route under the parent's
/// router — `POST /api/v1/{dom}/{parent}/{parent_id}/{child}/{child_id}/
/// actions/{op}` — with a scoped record load, the child command plane,
/// OpenAPI registration, and hurl coverage. Child `save`/`delete`
/// delegations lower via the scoped load; everything else stays a 501 seam.
#[tokio::test]
async fn child_service_ops_emit_nested_routes_and_scoped_loads() {
    let fixture = write_fixture_in("service_ops_child_ops", PAYROLL_DDD);
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

    // ── router: the child routes fn carries the action routes and the
    //    PARENT nests it; the child is NOT mounted at top level ──
    let router = read(&out.path().join("src/api/payroll/router.rs"));
    assert!(
        router.contains(".nest(\"/{pay_run_id}/pay-run-line\", pay_run_line_routes())"),
        "the parent router must nest the child routes fn:\n{router}"
    );
    let child_router_start = router
        .find("fn pay_run_line_routes()")
        .expect("the child routes fn must emit");
    let child_router = &router[child_router_start..];
    for snake in ["adjust", "reprice", "remove", "summarize"] {
        assert!(
            child_router.contains(&format!("/actions/{snake}\"")),
            "child router must register actions/{snake}:\n{child_router}"
        );
    }
    assert!(
        !router.contains(".nest(\"/pay-run-line\""),
        "the child routes fn must not mount at top level:\n{router}"
    );

    // ── child handler: nested utoipa path, two-uuid Path tuple, scoped load ──
    let handler = read(&out.path().join("src/api/payroll/pay_run_line_handler.rs"));
    // The nested action path with BOTH path params.
    assert!(
        handler.contains(
            "/api/v1/payroll/pay-run/{pay_run_id}/pay-run-line/{pay_run_line_id}/actions/adjust"
        ),
        "the child action utoipa path must nest under the parent:\n{handler}"
    );
    // The two-uuid Path tuple (depth-1 child; no grandparent segment).
    assert!(
        handler.contains("Path((parent_id, pay_run_line_id)): Path<(Uuid, Uuid)>"),
        "child action handlers must bind the parent + child ids:\n{handler}"
    );
    // The record load rides the SCOPED read (only emitted for children with
    // a parent_ref) — never the plain find_by_id.
    assert!(
        handler.contains("find_by_id_scoped(pay_run_line_id, parent_id"),
        "the child record load must be parent-scoped:\n{handler}"
    );
    // save → the child update command; delete → the child delete command.
    assert!(
        handler.contains("pay_run_line_commands.update(\n        pay_run_line_id,"),
        "the child save delegation must lower to the child command plane:\n{handler}"
    );
    assert!(
        handler.contains("_commands.delete(pay_run_line_id,"),
        "the child delete delegation must lower to the child command plane:\n{handler}"
    );
    // The post-update reload is scoped too (the child design is auditable
    // by default — the Sculptor semantics — so the `false` include_deleted
    // arg rides along).
    assert!(
        handler.contains("find_by_id_scoped(pay_run_line_id, parent_id, false, api_key_info"),
        "the post-update reload must be scoped:\n{handler}"
    );
    // The capability-gated child op renders its authorize fn.
    assert!(
        handler.contains("fn service_op_reprice_authorize"),
        "the capability-gated child op must render its authorize fn:\n{handler}"
    );
    // The findAll delegation keeps the 501 seam fail-closed.
    assert!(
        handler.contains("has no executable lowering"),
        "the unlowered child delegation must stay a 501 seam:\n{handler}"
    );
    // The child response type resolves through the child's DTO module.
    assert!(
        handler.contains("crate::domain::payroll::pay_run_line::dto_response::PayRunLineResponse"),
        "the child authorize fn must reference the child response type:\n{handler}"
    );

    // ── openapi: the child op registers (path-agnostic fn+module context) ──
    let openapi = read(&out.path().join("src/api/openapi/payroll.rs"));
    assert!(
        openapi.contains("pay_run_line_handler::service_op_adjust"),
        "openapi must register the child action paths:\n{openapi}"
    );

    // ── hurl: the child's actions contract is nested ──
    let child_actions = read(&out.path().join("hurl/10_payroll_pay_run_line_actions.hurl"));
    // The parent preamble captures the parent id first.
    assert!(
        child_actions.contains("POST {{base_url}}/api/v1/payroll/pay-run"),
        "the child actions contract must create the parent first:\n{child_actions}"
    );
    assert!(
        child_actions.contains("pay_run_id: jsonpath \"$.data.id\""),
        "the parent preamble must capture the parent id:\n{child_actions}"
    );
    // The child create runs at the nested URL and captures the child id.
    assert!(
        child_actions
            .contains("POST {{base_url}}/api/v1/payroll/pay-run/{{pay_run_id}}/pay-run-line"),
        "the child create must POST the nested collection:\n{child_actions}"
    );
    assert!(
        child_actions.contains("pay_run_line_id: jsonpath \"$.data.id\""),
        "the child create must capture the child id:\n{child_actions}"
    );
    // The action legs POST the nested action route.
    assert!(
        child_actions.contains(
            "{{base_url}}/api/v1/payroll/pay-run/{{pay_run_id}}/pay-run-line/{{pay_run_line_id}}/actions/adjust"
        ),
        "the child action legs must ride the nested action route:\n{child_actions}"
    );
    assert_eq!(
        status_after(&child_actions, "/actions/adjust"),
        Some(200),
        "{child_actions}"
    );
    assert_eq!(
        status_after(&child_actions, "/actions/remove"),
        Some(200),
        "{child_actions}"
    );
    assert_eq!(
        status_after(&child_actions, "/actions/summarize"),
        Some(501),
        "{child_actions}"
    );
    // The capability-gated child op rides the api_key bypass → 200.
    assert_eq!(
        status_after(&child_actions, "/actions/reprice"),
        Some(200),
        "{child_actions}"
    );

    // ── hurl: the child's ENTITY contract is nested too ──
    let child_entity = read(&out.path().join("hurl/10_payroll_pay_run_line.hurl"));
    assert!(
        child_entity.contains(
            "GET {{base_url}}/api/v1/payroll/pay-run/{{pay_run_id}}/pay-run-line?page=0&page_size=10"
        ),
        "the child entity contract's LIST must ride the nested collection:\n{child_entity}"
    );
    assert!(
        child_entity.contains(
            "{{base_url}}/api/v1/payroll/pay-run/{{pay_run_id}}/pay-run-line/{{pay_run_line_id}}"
        ),
        "the child entity contract's item URLs must nest under the parent:\n{child_entity}"
    );
    // The zero-uuid 404 check rides the nested URL (parent exists, child doesn't).
    assert!(
        child_entity.contains(
            "GET {{base_url}}/api/v1/payroll/pay-run/{{pay_run_id}}/pay-run-line/00000000-0000-0000-0000-000000000000"
        ),
        "the zero-uuid 404 check must stay nested:\n{child_entity}"
    );

    // The parent's own entity contract stays top-level.
    let parent_entity = read(&out.path().join("hurl/11_payroll_pay_run.hurl"));
    assert!(
        parent_entity.contains("GET {{base_url}}/api/v1/payroll/pay-run?page=0&page_size=10"),
        "the parent entity contract must stay top-level:\n{parent_entity}"
    );
}

/// The openapi service-op registration works in MOX-FIRST mode too: no
/// `entities` key in domains.toml, no JSON schemas — the pre-WS4 loop
/// iterated `domain_entry.entities`, which is empty there, so service ops
/// got NO OpenAPI registration. The generation-order loop registers both
/// entities' ops.
#[tokio::test]
async fn openapi_registers_service_ops_in_mox_first_mode() {
    let fixture = write_mox_first_fixture_in("service_ops_mox_first_openapi", PAYROLL_DDD);
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

    let openapi = read(&out.path().join("src/api/openapi/payroll.rs"));
    assert!(
        openapi.contains("pay_run_handler::service_op_approve"),
        "mox-first mode must register ROOT service ops:\n{openapi}"
    );
    assert!(
        openapi.contains("pay_run_line_handler::service_op_adjust"),
        "mox-first mode must register CHILD service ops:\n{openapi}"
    );
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
    fs::write(
        json_dir.join("PayRunLineType.json"),
        fixture::PAY_RUN_LINE_SCHEMA,
    )
    .unwrap();
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
