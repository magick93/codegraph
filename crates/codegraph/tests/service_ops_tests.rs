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
    DOMAINS_TOML, Fixture, PAY_RUN_MOX, PAY_RUN_SCHEMA, PAYROLL_DDD, RELEASE_DDD,
    guard_fixture_with_ambiguous_fk, guard_fixture_with_deep_path, guard_fixture_without_fk,
    ingest, ingest_authored_boundary_graph, ingest_binding, ingest_child_ops, ingest_guard, read,
    run_generation, run_generation_result, tempdir, write_binding_fixture,
    write_binding_fixture_with, write_child_ops_fixture, write_child_ops_fixture_with,
    write_child_ops_fixture_with_sources, write_fixture_in, write_guard_fixture,
    write_mox_first_fixture_in,
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

/// The one emitted actions contract whose file name ends with `suffix`
/// (`_release_actions.hurl`) — the fixture's numbering is discovered, never
/// pinned (the generation order owns it).
fn actions_file(output: &Path, suffix: &str) -> PathBuf {
    fs::read_dir(output.join("hurl"))
        .expect("hurl dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with(suffix))
        })
        .unwrap_or_else(|| panic!("no hurl file ending in `{suffix}` in {output:?}"))
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
        .join("migrations/000011_common_pay_run_status_codelist.sql");
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
    let actions = read(&out.path().join("hurl/12_payroll_pay_run_actions.hurl"));
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

/// Issue #662: a checked `.ddd` statement body lowers INTO the handler
/// (the `Body` execution kind), replacing the 501 seam for that op. The
/// fixture's `applyPayRun` is the consumer-critical `save`-payload shape:
/// `let … else NotFound` binds the addressed aggregate, `guard … else
/// Conflict` answers a REAL 4xx naming the declared error, the feature-path
/// assignment records the DTO wire key, and `self.save(..)` flushes it
/// through the command plane.
#[tokio::test]
async fn statement_bodies_lower_into_the_action_handler() {
    let fixture = write_fixture_in("service_ops_stmt_bodies", PAYROLL_DDD);
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

    let handler = read(&out.path().join("src/api/payroll/pay_run_handler.rs"));
    // The pre-rendered body block exists and names the op.
    assert!(
        handler.contains("statement body for `applyPayRun`"),
        "the body must lower into the handler:\n{handler}"
    );
    // Action-body parameter extraction (the declared primitive parameters).
    assert!(
        handler.contains("let total: i32 = body.as_ref()"),
        "the body's `int` parameter must bind as i32:\n{handler}"
    );
    // A parameter the body never reads still validates its presence, then
    // binds `_` (no dead-name warning in the handler).
    assert!(
        handler.contains("let _: String = body.as_ref()"),
        "an unread parameter must bind `_` after validating presence:\n{handler}"
    );
    assert!(
        handler.contains("missing or invalid parameter `note`"),
        "the required parameters must 400 when absent:\n{handler}"
    );
    // `let … else NotFound` binds the route's loaded aggregate.
    assert!(
        handler.contains("let mut run = response.clone();"),
        "the aggregate local must bind the loaded record (mut: it is assigned):\n{handler}"
    );
    // `guard … else Conflict` is a REAL 4xx naming the declared error.
    assert!(
        handler.contains("if !((run.total < 100))"),
        "the guard must lower as a negated condition:\n{handler}"
    );
    assert!(
        handler.contains(
            "AppError::conflict(format!(\"Conflict: {}\", \"total exceeds the action limit\"))"
        ) || handler.contains("total exceeds the action limit"),
        "the guard's raise must name the declared error and its payload:\n{handler}"
    );
    // Feature-path assignment: the typed write + the DTO wire key.
    assert!(
        handler.contains("run.total = total;"),
        "the assignment must write the loaded record:\n{handler}"
    );
    assert!(
        handler.contains("patch.insert(\"total\".to_string(), serde_json::json!(&run.total));"),
        "the assignment must record the DTO wire key:\n{handler}"
    );
    // `self.save(..)` flushes the patch through the command plane and the
    // response is the RELOADED record.
    assert!(
        handler.contains("_commands.update("),
        "the save must ride the update command:\n{handler}"
    );
    assert!(
        handler.contains("UpdatePayRunRequest"),
        "the save must build the entity's Update DTO:\n{handler}"
    );
    assert!(
        handler.contains("let data = latest.unwrap_or(response);"),
        "the response must be the RELOADED record (a save happened):\n{handler}"
    );
    assert!(
        handler.contains("state.payroll_pay_run_queries.find_by_id(pay_run_id, false"),
        "the save must reload through the addressed read:\n{handler}"
    );
    // The bodied op is no longer a seam.
    assert!(
        !handler.contains("`applyPayRun` has no delegation target"),
        "a bodied op must not answer the abstract seam:\n{handler}"
    );
    // utoipa documents the declared raises.
    assert!(
        handler.contains("description = \"The operation body raised Conflict\""),
        "the 409 response must document the declared raise:\n{handler}"
    );
    assert!(
        handler.contains("description = \"The operation body raised NotFound\""),
        "the 404 response must document the declared raise:\n{handler}"
    );

    // hurl: the action leg carries the declared parameters as its body and
    // pins 200 (the body has no 4xx-raising path... its guard DOES raise, so
    // the deterministic leg is omitted — the live gate drives it).
    let actions = read(&out.path().join("hurl/12_payroll_pay_run_actions.hurl"));
    assert!(
        actions.contains("A lowered `.ddd` statement body (issue #662)"),
        "a body-carrying entity documents the body plane in its contract:\n{actions}"
    );
    assert!(
        !actions.contains("/actions/apply_pay_run"),
        "a guard-carrying body has no deterministic expected status, so the \
         emitted contract must not pin one:\n{actions}"
    );

    // The router still mounts the route.
    let router = read(&out.path().join("src/api/payroll/router.rs"));
    assert!(
        router.contains("/actions/apply_pay_run\""),
        "the bodied op's route must mount:\n{router}"
    );
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
    let child_actions = read(&out.path().join("hurl/11_payroll_pay_run_line_actions.hurl"));
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
    let child_entity = read(&out.path().join("hurl/11_payroll_pay_run_line.hurl"));
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
    let parent_entity = read(&out.path().join("hurl/12_payroll_pay_run.hurl"));
    assert!(
        parent_entity.contains("GET {{base_url}}/api/v1/payroll/pay-run?page=0&page_size=10"),
        "the parent entity contract must stay top-level:\n{parent_entity}"
    );
}

/// Depth-2 GRANDCHILD service ops (issue #629 — the #551 gap): a
/// refers-linked child of the CHILD with its own repository emits action
/// routes through the grandparent branch —
/// `POST /api/v1/{dom}/{parent}/{pid}/{child}/{cid}/{grandchild}/{gid}/
/// actions/{op}` — with a three-uuid Path tuple, a scoped record load, and
/// hurl contracts that capture the full nesting chain. The surface rendered
/// via the existing depth-2 branch since WS4 but compiled NOWHERE before
/// the `service_ops_compile_gate` extension.
#[tokio::test]
async fn grandchild_service_ops_emit_depth2_routes_and_scoped_loads() {
    let fixture = write_fixture_in("service_ops_grandchild_ops", PAYROLL_DDD);
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

    // ── router: the CHILD routes fn nests the grandchild; the grandchild is
    //    not mounted at top level or directly under the parent ──
    let router = read(&out.path().join("src/api/payroll/router.rs"));
    assert!(
        router.contains(
            ".nest(\"/{pay_run_line_id}/pay-run-line-adjustment\", pay_run_line_adjustment_routes())"
        ),
        "the child router must nest the grandchild routes fn:\n{router}"
    );
    let grandchild_router_start = router
        .find("fn pay_run_line_adjustment_routes()")
        .expect("the grandchild routes fn must emit");
    let grandchild_router = &router[grandchild_router_start..];
    for snake in ["recalculate", "void_out"] {
        assert!(
            grandchild_router.contains(&format!("/actions/{snake}\"")),
            "grandchild router must register actions/{snake}:\n{grandchild_router}"
        );
    }

    // ── handler: depth-2 utoipa path, three-uuid Path tuple, scoped load ──
    let handler = read(
        &out.path()
            .join("src/api/payroll/pay_run_line_adjustment_handler.rs"),
    );
    assert!(
        handler.contains(
            "/api/v1/payroll/pay-run/{pay_run_id}/pay-run-line/{pay_run_line_id}/pay-run-line-adjustment/{pay_run_line_adjustment_id}/actions/recalculate"
        ),
        "the grandchild action utoipa path must nest under BOTH ancestors:\n{handler}"
    );
    assert!(
        handler.contains(
            "Path((_grandparent_id, parent_id, pay_run_line_adjustment_id)): Path<(Uuid, Uuid, Uuid)>"
        ),
        "grandchild action handlers must bind all three ids (the grandparent\n\
         positionally as _grandparent_id, the parent as parent_id):\n{handler}"
    );
    assert!(
        handler.contains("fn service_op_recalculate_authorize"),
        "the capability-gated grandchild op must render its authorize fn:\n{handler}"
    );
    // save → the grandchild update command; delete → the grandchild delete
    // command (the wired delegation legs at depth 2).
    assert!(
        handler.contains("_commands.update("),
        "the grandchild save delegation must lower to the command plane:\n{handler}"
    );
    assert!(
        handler.contains("_commands.delete("),
        "the grandchild delete delegation must lower to the command plane:\n{handler}"
    );
    // The record load rides the SCOPED read — never a plain find_by_id.
    assert!(
        handler.contains("find_by_id_scoped(pay_run_line_adjustment_id,"),
        "the grandchild record load must be parent-scoped:\n{handler}"
    );
    // The authorize fn references the grandchild's DTO module.
    assert!(
        handler.contains(
            "crate::domain::payroll::pay_run_line_adjustment::dto_response::PayRunLineAdjustmentResponse"
        ),
        "the grandchild authorize fn must reference the grandchild response type:\n{handler}"
    );

    // ── openapi: the grandchild op registers ──
    let openapi = read(&out.path().join("src/api/openapi/payroll.rs"));
    assert!(
        openapi.contains("pay_run_line_adjustment_handler::service_op_recalculate"),
        "openapi must register the grandchild action paths:\n{openapi}"
    );

    // ── hurl: the grandchild's contracts capture the FULL nesting chain ──
    let grandchild_actions = read(
        &out.path()
            .join("hurl/10_payroll_pay_run_line_adjustment_actions.hurl"),
    );
    // Grandparent + child preambles: two captures before the action legs.
    assert!(
        grandchild_actions.contains("POST {{base_url}}/api/v1/payroll/pay-run"),
        "the grandchild actions contract must create the grandparent first:\n{grandchild_actions}"
    );
    assert!(
        grandchild_actions.contains("pay_run_id: jsonpath \"$.data.id\""),
        "the grandparent preamble must capture its id:\n{grandchild_actions}"
    );
    assert!(
        grandchild_actions
            .contains("POST {{base_url}}/api/v1/payroll/pay-run/{{pay_run_id}}/pay-run-line"),
        "the child preamble must POST the nested collection:\n{grandchild_actions}"
    );
    assert!(
        grandchild_actions.contains("pay_run_line_id: jsonpath \"$.data.id\""),
        "the child preamble must capture its id:\n{grandchild_actions}"
    );
    assert!(
        grandchild_actions.contains(
            "{{base_url}}/api/v1/payroll/pay-run/{{pay_run_id}}/pay-run-line/{{pay_run_line_id}}/pay-run-line-adjustment/{{pay_run_line_adjustment_id}}/actions/recalculate"
        ),
        "the grandchild action legs must ride the depth-2 action route:\n{grandchild_actions}"
    );
    assert_eq!(
        status_after(&grandchild_actions, "/actions/recalculate"),
        Some(200),
        "{grandchild_actions}"
    );
    assert_eq!(
        status_after(&grandchild_actions, "/actions/void_out"),
        Some(200),
        "{grandchild_actions}"
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
                .join("migrations/000011_common_pay_run_status_codelist.sql"),
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
// phero-driver typechecker already rejects most of the out-of-subset
// surface before the graph, so the graph-side refusal is defense-in-depth
// for `.json` actor artifacts.

/// Issue #662: a body construct outside the closed subset is a HARD
/// pre-generation error naming the op and the construct (the DD9
/// `query_refusals` posture) — never a 501 seam, never a partially
/// interpreted handler.
#[tokio::test]
async fn unlowerable_body_constructs_fail_validation_naming_the_op() {
    let source = fixture::payroll_ddd_with_refused_body();
    let fixture = write_fixture_in("service_ops_body_refusal", &source);
    let engine = ingest(&fixture).await;
    let config = codegraph_config::config::parse_domain_config(&fixture.config).unwrap();
    let issues = codegraph::validate::ValidationPass::run(&engine, &config).await;
    let refusal = issues
        .iter()
        .find(|i| {
            matches!(i.severity, codegraph::validate::Severity::Error)
                && i.check == "service_op_body"
        })
        .unwrap_or_else(|| panic!("expected a hard body refusal, got: {issues:?}"));
    assert!(
        refusal.message.contains("applyPayRun"),
        "the refusal must name the op: {refusal:?}"
    );
    assert!(
        refusal
            .message
            .contains("binary expression in a value slot"),
        "the refusal must name the construct: {refusal:?}"
    );
}

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
async fn injected_scope_when_guards_load_the_related_record_and_deny() {
    // Issue #663: an op-level `when` over an INJECTED repository's field
    // lowers to a real guard — the handler loads the related record through
    // the repository plane's own query and denies with the rule named.
    let fixture = write_guard_fixture();
    let engine = ingest_guard(&fixture).await;

    // The resolved condition paths survive ingest: the injected scope names
    // the declared dependency (`TimecardRepository`) and the class its
    // design binds.
    let models = engine.get_ddd_models().await.unwrap();
    let op = models
        .iter()
        .flat_map(|m| &m.services)
        .flat_map(|s| &s.operations)
        .find(|op| op.name == "submit")
        .expect("submit op");
    assert!(
        !op.condition_paths.is_empty(),
        "the injected-scope guard must carry condition_paths"
    );
    let injected = op
        .condition_paths
        .iter()
        .find(|p| {
            matches!(
                p.scope,
                codegraph_core::types::DddConditionScope::Injected { .. }
            )
        })
        .expect("an injected-scope path");
    let codegraph_core::types::DddConditionScope::Injected { repository, class } = &injected.scope
    else {
        unreachable!("found above");
    };
    assert_eq!(repository, "TimecardRepository");
    assert!(class.ends_with("TimecardType"), "class: {class}");
    assert_eq!(injected.field, "timecardStatus");

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
    assert!(
        !report.has_errors(),
        "the injected-scope guard must lower: {report:?}"
    );

    // ── handler: the async authorize fn loads the related record ──
    let handler = read(&out.path().join("src/api/guardpay/pay_run_handler.rs"));
    // The injected load makes the gate async and hands it the state +
    // session (the sibling domain's query slot and the RLS context).
    assert!(
        handler.contains("async fn service_op_submit_authorize"),
        "the injected guard makes the authorize fn async:\n{handler}"
    );
    let start = handler
        .find("fn service_op_submit_authorize")
        .expect("submit authorize fn");
    assert!(
        handler.contains("state: &AppState"),
        "the gate takes AppState for the sibling query slot:\n{}",
        &handler[start..]
    );
    // The own-record FK carries the id (scalar `refers` → `timecard_id`).
    assert!(
        handler.contains("response.timecard_id"),
        "the id comes from the own-record FK:\n{}",
        &handler[start..]
    );
    // The load rides the INJECTED entity's own query slot.
    assert!(
        handler.contains("state.guardpay_timecard_queries"),
        "the load rides the sibling query slot:\n{}",
        &handler[start..]
    );
    // The comparison is codelist-Display-based over the RELATED record.
    assert!(
        handler.contains(r#"timecard_repository.timecard_status.to_string() == "Approved""#),
        "the injected comparison lowers over the loaded record:\n{}",
        &handler[start..]
    );
    // Business semantics apply to every session: the guard precedes the
    // api_key bypass (the #590 contract).
    let guard_pos = handler[start..]
        .find("op-level `when` transition guard")
        .expect("guard comment");
    let bypass_pos = handler[start..]
        .find(r#"if role == "api_key""#)
        .expect("api_key bypass");
    assert!(
        guard_pos < bypass_pos,
        "the injected guard must precede the api_key bypass:\n{}",
        &handler[start..]
    );
    // The route still wires the transition (the guard composes, it does not
    // replace the machine).
    assert!(
        handler.contains("fn service_op_submit("),
        "the route handler exists:\n{handler}"
    );

    // ── hurl: the fresh fixture cannot drive the Timecard past its
    // initial state, so the contract pins the conservative denial leg and
    // says why.
    let actions_dir = out.path().join("hurl");
    let mut actions = None;
    let entries = std::fs::read_dir(&actions_dir).expect("hurl dir");
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("hurl")
            && std::fs::read_to_string(&path)
                .map(|c| c.contains("/actions/submit"))
                .unwrap_or(false)
        {
            actions = Some(std::fs::read_to_string(path).expect("actions file"));
        }
    }
    let actions = actions.expect("the submit actions contract");
    // The leg's comment (the `note`) renders ABOVE the POST line — the
    // whole-file search finds it; the status pins inside the leg slice.
    assert!(
        actions.contains("injected-scope `when` guard"),
        "the leg's comment must say why the denial is pinned:\n{actions}"
    );
    assert!(
        actions.contains("TimecardRepository.timecardStatus == Approved"),
        "the leg's comment must name the guard it pins:\n{actions}"
    );
    let leg = actions
        .find("/actions/submit")
        .map(|pos| &actions[pos..])
        .expect("submit leg");
    assert!(
        leg.contains("HTTP 403"),
        "the fresh fixture cannot satisfy the injected guard — the contract \
         pins the denial:\n{actions}"
    );
}

#[tokio::test]
async fn multi_machine_module_accepts_op_level_when() {
    // Issue #663: the single-machine-module restriction on op-level `when`
    // lifted upstream (pherolang#180) — a module with two machine-bearing
    // designs accepts an op-level `when`, whose own-record path rides the
    // machine field the entity owns.
    let source = RELEASE_DDD.replace(
        "prepareRelease => ReleaseRepository.save transition ReleaseStatus.SignOff;",
        "prepareRelease => ReleaseRepository.save transition ReleaseStatus.SignOff when (releaseStatus == Held);",
    );
    let fixture = write_binding_fixture_with(&source);
    let engine = ingest_binding(&fixture).await;

    // Multi-machine module: the op's condition_paths carry the own-record
    // machine field (the upstream contract — own-record paths are emitted
    // exactly where the owning machine is not a priori derivable).
    let models = engine.get_ddd_models().await.unwrap();
    let op = models
        .iter()
        .flat_map(|m| &m.services)
        .flat_map(|s| &s.operations)
        .find(|op| op.name == "prepareRelease")
        .expect("prepareRelease op");
    assert!(
        op.condition_paths.iter().any(|p| matches!(
            p.scope,
            codegraph_core::types::DddConditionScope::OwnRecord
        ) && p.field == "releaseStatus"),
        "the own-record machine path must ride condition_paths: {:?}",
        op.condition_paths
    );

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
    assert!(
        !report.has_errors(),
        "a multi-machine module must accept the op-level `when`: {report:?}"
    );
    let handler = read(&out.path().join("src/api/release/release_handler.rs"));
    assert!(
        handler.contains(r#"release_status.to_string() == "Held""#),
        "the own-record guard lowers over the loaded record:\n{handler}"
    );
}

#[tokio::test]
async fn injected_guard_without_fk_is_a_named_generation_error() {
    // The injected root names a class the op entity does not reference:
    // the guard has no id source. A named HARD validation error — never a
    // load of an unresolvable record, never a silently narrower guard.
    let fixture = guard_fixture_without_fk();
    let engine = ingest_guard(&fixture).await;
    let config = codegraph_config::config::parse_domain_config(&fixture.config).unwrap();
    let issues = codegraph::validate::ValidationPass::run(&engine, &config).await;
    let refusal = issues
        .iter()
        .find(|i| {
            matches!(i.severity, codegraph::validate::Severity::Error)
                && i.check == "service_op_guard"
        })
        .unwrap_or_else(|| panic!("the missing FK must hard-fail validation: {issues:?}"));
    assert!(
        refusal.message.contains("TimecardRepository") && refusal.message.contains("no id source"),
        "the error must name the injection and the missing FK feature: {refusal:?}"
    );
}

#[tokio::test]
async fn injected_guard_with_ambiguous_fk_is_a_named_generation_error() {
    let fixture = guard_fixture_with_ambiguous_fk();
    let engine = ingest_guard(&fixture).await;
    let config = codegraph_config::config::parse_domain_config(&fixture.config).unwrap();
    let issues = codegraph::validate::ValidationPass::run(&engine, &config).await;
    let refusal = issues
        .iter()
        .find(|i| {
            matches!(i.severity, codegraph::validate::Severity::Error)
                && i.check == "service_op_guard"
        })
        .unwrap_or_else(|| panic!("the ambiguous FK must hard-fail validation: {issues:?}"));
    assert!(
        refusal.message.contains("timecard_id") && refusal.message.contains("review_timecard_id"),
        "the error must name both candidate FK fields: {refusal:?}"
    );
}

#[tokio::test]
async fn injected_guard_with_dotted_path_is_a_named_generation_error() {
    // `TimecardRepository.holder.name == "Ada"` compiles upstream (each
    // segment is a stored field, the refer required so R3 is satisfied) but
    // is OUTSIDE the lowering's closed subset: one segment under the
    // injected root only.
    let fixture = guard_fixture_with_deep_path();
    let engine = ingest_guard(&fixture).await;
    let config = codegraph_config::config::parse_domain_config(&fixture.config).unwrap();
    let issues = codegraph::validate::ValidationPass::run(&engine, &config).await;
    let refusal = issues
        .iter()
        .find(|i| {
            matches!(i.severity, codegraph::validate::Severity::Error)
                && i.check == "service_op_guard"
        })
        .unwrap_or_else(|| panic!("the dotted path must hard-fail validation: {issues:?}"));
    assert!(
        refusal.message.contains("holder.name"),
        "the error must name the construct: {refusal:?}"
    );
}

#[tokio::test]
async fn injected_guards_refuse_under_workers_topology() {
    // Issue #663 review fix: under `deployment_topology = "workers"` the
    // #590 action handlers run in per-domain worker crates whose AppState
    // carries only its OWN domain's query slots — a cross-domain injected
    // load renders a field no worker crate has (E0599 at consumer build,
    // silently). The plane refuses (named error) instead of breaking the
    // worker; the monolith keeps lowering (the sibling test).
    let fixture = write_guard_fixture();
    let engine = ingest_guard(&fixture).await;
    let config = codegraph_config::config::parse_domain_config(&fixture.config).unwrap();
    let body = codegraph_generate::ddd::service_ops::BodyRenderContext {
        workers_topology: true,
        ..Default::default()
    };
    let err = codegraph_generate::ddd::service_ops::routes_for_entity(
        &engine,
        &config,
        "guardpay",
        "PayRunType",
        &body,
    )
    .await
    .expect_err("the workers topology must refuse injected guards");
    assert!(
        err.contains("workers") && err.contains("injected-scope `when` guard"),
        "the refusal must name the topology and the guard: {err}"
    );
    // The monolith (default context) lowers.
    let routes = codegraph_generate::ddd::service_ops::routes_for_entity(
        &engine,
        &config,
        "guardpay",
        "PayRunType",
        &Default::default(),
    )
    .await
    .expect("the monolith lowers the injected guard");
    assert!(
        routes
            .iter()
            .any(|route| route.authorize_async && route.injected_guard.is_some()),
        "the monolith leg must keep the async guard: {routes:?}"
    );
    // The refusal rides the PRE-GENERATION validation pass (issue #663
    // review round 2): with the topology threaded, `run_with_topology`
    // hard-fails BEFORE any tree is written; without it the pass is blind
    // and a workers run would exit 0 with the action surface missing.
    let issues =
        codegraph::validate::ValidationPass::run_with_topology(&engine, &config, true).await;
    assert!(
        issues.iter().any(|i| {
            matches!(i.severity, codegraph::validate::Severity::Error)
                && i.check == "service_op_guard"
                && i.message.contains("workers")
        }),
        "the threaded validation pass must surface the workers refusal: {issues:?}"
    );
    let issues =
        codegraph::validate::ValidationPass::run_with_topology(&engine, &config, false).await;
    assert!(
        !issues
            .iter()
            .any(|i| i.check == "service_op_guard" && i.message.contains("workers")),
        "the monolith pass must not refuse: {issues:?}"
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

// ── Issue #660: the authored `transition` binding is the selector ───────

/// The binding mirrored off the design is the primary selector: a
/// `name => Repository.save transition <EnumType>.<Trigger>` operation
/// applies EXACTLY the declared `from -> to`, whatever its name looks like,
/// and the emitted state-aware hurl contract simulates the machine off it
/// in declaration order (the guaranteed wrong-state replay and the 501 seam
/// stay).
///
/// The fixture's two machine-bearing designs in ONE module force the
/// qualified spelling upstream, so this also pins the `TransitionRef.machine`
/// resolution: `ReleaseStatus.SignOff` selects the `releaseStatus` machine,
/// `ReviewStatus.Clear` the `reviewStatus` one.
#[tokio::test]
async fn authored_transition_binding_selects_the_declared_transition() {
    let fixture = write_binding_fixture();
    let engine = ingest_binding(&fixture).await;
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

    // The binding survives ingest → graph → readback as authored (issue
    // #660): trigger + enum-type qualifier, so the planner never re-derives
    // it from names.
    let models = engine.get_ddd_models().await.unwrap();
    let binding = models
        .iter()
        .flat_map(|m| &m.services)
        .flat_map(|s| &s.operations)
        .find(|o| o.name == "prepareRelease")
        .and_then(|o| o.transition.clone())
        .expect("the authored binding must mirror into the graph");
    assert_eq!(binding.trigger, "SignOff");
    assert_eq!(binding.machine.as_deref(), Some("ReleaseStatus"));
    let unbound = models
        .iter()
        .flat_map(|m| &m.services)
        .flat_map(|s| &s.operations)
        .find(|o| o.name == "inspect")
        .expect("the read op");
    assert!(
        unbound.transition.is_none(),
        "a delegation without a binding must mirror as None"
    );

    // ── routes: both entities' action planes emit ──
    let router = read(&out.path().join("src/api/release/router.rs"));
    for snake in ["prepare_release", "finalize", "prepare_review"] {
        assert!(
            router.contains(&format!("/actions/{snake}\"")),
            "router must register actions/{snake}:\n{router}"
        );
    }

    // ── handler: the DECLARED transition pair, nothing inferred ──
    let handler = read(&out.path().join("src/api/release/release_handler.rs"));
    assert!(
        handler.contains(r#""Held" => Some("SignedOff")"#),
        "`prepareRelease` must apply its authored Held -> SignedOff:\n{handler}"
    );
    assert!(
        handler.contains(r#""SignedOff" => Some("Released")"#),
        "`finalize` must apply its authored SignedOff -> Released:\n{handler}"
    );
    // The op names resemble no trigger; a heuristic would have selected
    // NOTHING here, so any other arm is a wrong inference.
    for inferred in [
        r#""Held" => Some("Released")"#,
        r#""SignedOff" => Some("SignedOff")"#,
    ] {
        assert!(
            !handler.contains(inferred),
            "an inferred arm leaked into the authored binding ({inferred}):\n{handler}"
        );
    }

    // ── the SECOND machine resolves through the enum-type qualifier ──
    let review = read(&out.path().join("src/api/release/review_handler.rs"));
    assert!(
        review.contains(r#""Pending" => Some("Cleared")"#),
        "`prepareReview` must apply its authored Pending -> Cleared:\n{review}"
    );

    // ── hurl: declaration order, exact legs ──
    let actions = read(&actions_file(out.path(), "_release_actions.hurl"));
    // The machine starts at Held: the first bound op advances to SignedOff,
    // the second from SignedOff to Released, the read answers 200, the
    // findAll delegation still seams 501.
    assert_eq!(
        status_after(&actions, "/actions/prepare_release"),
        Some(200),
        "{actions}"
    );
    assert_eq!(
        status_after(&actions, "/actions/finalize"),
        Some(200),
        "{actions}"
    );
    assert_eq!(
        status_after(&actions, "/actions/inspect"),
        Some(200),
        "{actions}"
    );
    assert_eq!(
        status_after(&actions, "/actions/summarize"),
        Some(501),
        "{actions}"
    );
    // The guaranteed replay: `finalize` (SignedOff -> Released) from the
    // post-sequence state Released has no transition.
    assert_eq!(
        status_after_last(&actions, "/actions/finalize"),
        Some(409),
        "the wrong-state replay must pin 409:\n{actions}"
    );
    assert!(
        actions.contains("wrong-state"),
        "the contract must document the replay leg:\n{actions}"
    );

    let review_actions = read(&actions_file(out.path(), "_review_actions.hurl"));
    assert_eq!(
        status_after(&review_actions, "/actions/prepare_review"),
        Some(200),
        "{review_actions}"
    );
    assert_eq!(
        status_after(&review_actions, "/actions/inspect_review"),
        Some(200),
        "{review_actions}"
    );
    assert_eq!(
        status_after_last(&review_actions, "/actions/prepare_review"),
        Some(409),
        "the review machine's replay must pin 409:\n{review_actions}"
    );
}

/// The no-binding fallback stays: PAYROLL_DDD declares no `transition`, so
/// every op the heuristics resolve warns ONCE naming the missing binding
/// (the run-path diagnostic), and an op that applies no transition at all
/// stays quiet.
#[tokio::test]
async fn unbound_machine_ops_warn_once_for_the_missing_binding() {
    let fixture = write_fixture_in("service_ops_transition_warn", PAYROLL_DDD);
    let engine = ingest(&fixture).await;
    let config = codegraph_config::config::parse_domain_config(&fixture.config).unwrap();
    let issues = codegraph::validate::ValidationPass::run(&engine, &config).await;
    let warnings: Vec<&codegraph::validate::ValidationIssue> = issues
        .iter()
        .filter(|i| {
            matches!(i.severity, codegraph::validate::Severity::Warning)
                && i.check == "service_op_transition_binding"
        })
        .collect();
    for op in ["submit", "approve", "reverse"] {
        let hit = warnings
            .iter()
            .find(|w| w.message.contains(&format!("PayRunService::{op}")))
            .unwrap_or_else(|| panic!("a WARN must name `{op}`: {issues:?}"));
        assert!(
            hit.message.contains("declares no `transition` binding"),
            "the warning must name the missing binding: {hit:?}"
        );
    }
    assert!(
        !warnings
            .iter()
            .any(|w| w.message.contains("::archive") || w.message.contains("::notify")),
        "an op applying no transition must not warn: {warnings:?}"
    );
    assert_eq!(
        warnings.len(),
        3,
        "exactly one WARN per heuristic-resolved op: {warnings:?}"
    );

    // A fully-bound model produces none of these warnings.
    let bound = write_binding_fixture();
    let bound_engine = ingest_binding(&bound).await;
    let bound_config = codegraph_config::config::parse_domain_config(&bound.config).unwrap();
    let bound_issues = codegraph::validate::ValidationPass::run(&bound_engine, &bound_config).await;
    assert!(
        !bound_issues
            .iter()
            .any(|i| i.check == "service_op_transition_binding"
                && matches!(i.severity, codegraph::validate::Severity::Warning)),
        "an authored binding must silence the fallback warning: {bound_issues:?}"
    );
}

/// An authored binding that cannot be resolved is a HARD pre-generation
/// error naming the op and the machines (issue #660) — never a silent
/// fallback to the trigger heuristics, never a half-written tree.
///
/// Upstream rejects an unqualified binding in a multi-machine module at
/// compile time, so this pin drives the class the graph-side check exists
/// for: a STALE/hand-built artifact (the same module re-ingested with the
/// qualifier dropped), which validates the same posture as the DD9
/// `query_refusals` and the #662 body refusals.
#[tokio::test]
async fn unresolvable_authored_bindings_fail_validation_naming_the_machines() {
    use codegraph_core::traits::GraphIngestor;

    let fixture = write_binding_fixture();
    let engine = ingest_binding(&fixture).await;
    let models = engine.get_ddd_models().await.unwrap();
    let mut stale = models
        .iter()
        .find(|m| m.source_path.ends_with("release.ddd"))
        .expect("the release model")
        .clone();
    stale.source_path = "stale_release.ddd".to_string();
    let op = stale
        .services
        .iter_mut()
        .find(|s| s.name == "ReleaseService")
        .and_then(|s| s.operations.iter_mut().find(|o| o.name == "prepareRelease"))
        .expect("the bound op");
    op.transition = Some(codegraph_core::types::DddTransitionRef {
        trigger: "SignOff".to_string(),
        machine: None,
    });
    engine.ingest_ddd_model(&stale).await.unwrap();

    let config = codegraph_config::config::parse_domain_config(&fixture.config).unwrap();
    let issues = codegraph::validate::ValidationPass::run(&engine, &config).await;
    let refusal = issues
        .iter()
        .find(|i| {
            matches!(i.severity, codegraph::validate::Severity::Error)
                && i.check == "service_op_transition_binding"
        })
        .unwrap_or_else(|| panic!("expected a hard binding refusal, got: {issues:?}"));
    assert!(
        refusal.message.contains("prepareRelease"),
        "the refusal must name the op: {refusal:?}"
    );
    for machine in ["ReleaseType", "ReviewType"] {
        assert!(
            refusal.message.contains(machine),
            "the refusal must name both machines of the module ({machine}): {refusal:?}"
        );
    }

    // The direct collector exposes the same finding (the run path's source).
    let errors = codegraph_generate::ddd::service_ops::transition_binding_errors(&engine, &config)
        .await
        .unwrap();
    assert!(
        errors.iter().any(|e| e.contains("prepareRelease")),
        "{errors:?}"
    );
}

// ── Issue #661: aggregate child operations (`children <Class>`) ────────

/// The ingest mirror: `Design.child_operations` lands on the design node
/// with each operation's `child_scope` (feature, resolved child class,
/// validated action), and SERVICE operations carry none.
#[tokio::test]
async fn aggregate_child_operations_ingest_with_their_child_scope() {
    let fixture = write_child_ops_fixture();
    let engine = ingest_child_ops(&fixture).await;

    let models = engine.get_ddd_models().await.unwrap();
    let design = models[0]
        .designs
        .iter()
        .find(|d| d.class == "PayRunType")
        .expect("the PayRunType design");
    assert_eq!(
        design
            .child_operations
            .iter()
            .map(|op| op.name.as_str())
            .collect::<Vec<_>>(),
        ["addLine", "saveLine", "removeLine"],
        "the `children PayRunType` block must lower onto the design in declaration order"
    );
    let add = &design.child_operations[0];
    let scope = add.child_scope.as_ref().expect("the child scope mirrors");
    assert_eq!(scope.feature, "lines");
    assert_eq!(scope.class, "payroll.PayRunLineType");
    assert_eq!(scope.action, "add");
    assert_eq!(
        add.params.len(),
        1,
        "the declared DTO parameter rides the mirror"
    );
    assert!(
        add.delegation_target.is_none(),
        "a child operation delegates to nothing — it carries `childScope`"
    );
    // Ordinary service operations never carry a child scope.
    let service_ops: Vec<&codegraph_core::types::DddServiceOperation> = models[0]
        .services
        .iter()
        .flat_map(|s| s.operations.iter())
        .collect();
    assert!(!service_ops.is_empty(), "the fixture has a root service");
    assert!(
        service_ops.iter().all(|op| op.child_scope.is_none()),
        "service operations carry no child scope"
    );
}

/// Acceptance (1) + (3): a modeled `children PayRunType { boolean
/// addLine(PayRunLineDraft line) => lines.add }` generates the nested child
/// action routes — against the CHILD's plane, under the parent's aggregate
/// path, with the parent ownership load the WS4 child plane already
/// generates — and the emitted actions contract exercises add → read-back
/// with its failure leg pinned.
#[tokio::test]
async fn child_scoped_operations_lower_to_nested_child_action_routes() {
    let fixture = write_child_ops_fixture();
    let engine = ingest_child_ops(&fixture).await;
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

    // ── router: the child routes fn carries the child actions; the parent
    //    nests it (the WS4 shape — no second nesting) ──
    let router = read(&out.path().join("src/api/payroll/router.rs"));
    assert!(
        router.contains(".nest(\"/{pay_run_id}/pay-run-line\", pay_run_line_routes())"),
        "the parent router must nest the child routes fn:\n{router}"
    );
    let child_router = &router[router
        .find("fn pay_run_line_routes()")
        .expect("the child routes fn must emit")..];
    assert!(
        child_router.contains("\"/actions/add_line\""),
        "`add` is a COLLECTION-level action (no child id in the path):\n{child_router}"
    );
    for (action, snake) in [("save", "save_line"), ("remove", "remove_line")] {
        assert!(
            child_router.contains(&format!("\"/{{pay_run_line_id}}/actions/{snake}\"")),
            "child `{action}` must ride the item route:\n{child_router}"
        );
    }

    // ── child handler: nested utoipa paths, the parent-scoped load, and the
    //    child command plane per action ──
    let handler = read(&out.path().join("src/api/payroll/pay_run_line_handler.rs"));
    assert!(
        handler.contains("/api/v1/payroll/pay-run/{pay_run_id}/pay-run-line/actions/add_line"),
        "the add utoipa path must be the nested collection action:\n{handler}"
    );
    assert!(
        handler.contains(
            "/api/v1/payroll/pay-run/{pay_run_id}/pay-run-line/{pay_run_line_id}/actions/save_line"
        ),
        "the save utoipa path must be the nested item action:\n{handler}"
    );
    assert!(
        handler.contains("Path((parent_id, pay_run_line_id)): Path<(Uuid, Uuid)>"),
        "item-level child actions bind the parent + child ids:\n{handler}"
    );
    assert!(
        handler.contains("Path(parent_id): Path<Uuid>,"),
        "the collection-level add binds the parent only:\n{handler}"
    );
    // add → the child create command, linked to the parent by the path.
    assert!(
        handler.contains("_commands.create(\n        item,\n        parent_id,"),
        "child `add` must create through the child's command plane, parent-linked:\n{handler}"
    );
    // save → the child update command; remove → the child delete command.
    assert!(
        handler.contains("_commands.update(\n        pay_run_line_id,"),
        "child `save` must update through the child's command plane:\n{handler}"
    );
    assert!(
        handler.contains("_commands.delete(pay_run_line_id,"),
        "child `remove` must delete through the child's command plane:\n{handler}"
    );
    // The scoped read is the parent ownership contract for item actions.
    assert!(
        handler.contains("find_by_id_scoped(pay_run_line_id, parent_id"),
        "item-level child actions must load the parent-scoped record:\n{handler}"
    );
    // DD3: the fixture's `dto PayRunLineDraft from PayRunLineType` projection
    // must NOT shadow the entity design on the same title — the design surface
    // still sees the entity design, so the audit plane (and its
    // `include_deleted` argument) survives.
    assert!(
        handler.contains("find_by_id_scoped(pay_run_line_id, parent_id, false, api_key_info"),
        "a projection DTO must not shadow the entity design (the auditable \
         `include_deleted` argument must survive):\n{handler}"
    );
    // None of the three child actions may seam.
    assert!(
        !handler.contains("child operation `addLine`")
            && !handler.contains("child operation `saveLine`"),
        "the modeled child actions must lower, not seam:\n{handler}"
    );
    // The RETAINED cross-aggregate seam carries the improved message naming
    // the modeling construct (acceptance 2) — it rides the ROOT handler.
    let root_handler = read(&out.path().join("src/api/payroll/pay_run_handler.rs"));
    assert!(
        root_handler.contains("children PayRunType"),
        "the cross-aggregate seam must name the `children <Root>` construct:\n{root_handler}"
    );
    assert!(
        root_handler.contains("childScope"),
        "the seam must name the childScope modeling construct:\n{root_handler}"
    );
    assert!(
        root_handler.contains("Refusing to patch the parent row silently"),
        "the seam keeps its never-silently-patch guarantee:\n{root_handler}"
    );

    // ── openapi: the child actions register through the child's module ──
    let openapi = read(&out.path().join("src/api/openapi/payroll.rs"));
    for snake in ["add_line", "save_line", "remove_line"] {
        assert!(
            openapi.contains(&format!("pay_run_line_handler::service_op_{snake}")),
            "openapi must register the child action `{snake}`:\n{openapi}"
        );
    }

    // ── hurl: create → add → read-back → failure leg → save/remove ──
    let actions = read(&actions_file(
        out.path(),
        "_payroll_pay_run_line_actions.hurl",
    ));
    assert!(
        actions.contains(
            "POST {{base_url}}/api/v1/payroll/pay-run/{{pay_run_id}}/pay-run-line/actions/add_line"
        ),
        "the add leg must post the nested collection action:\n{actions}"
    );
    assert_eq!(
        status_after(&actions, "/actions/add_line"),
        Some(200),
        "{actions}"
    );
    assert!(
        actions.contains("added_add_line_id: jsonpath \"$.data.id\""),
        "the add leg must capture the created element:\n{actions}"
    );
    // Issue #692: the bogus-parent 404 failure leg — the SAME collection
    // URL with the ZERO-UUID PARENT segment, the child's create body,
    // expect 404 (the parent load is the route's 404 contract; before the
    // fix a bogus parent reached the FK and answered 500).
    let bogus_parent_url = "{{base_url}}/api/v1/payroll/pay-run/00000000-0000-0000-0000-000000000000/pay-run-line/actions/add_line";
    assert!(
        actions.contains(bogus_parent_url),
        "the add failure leg must probe a bogus parent through the collection route:\n{actions}"
    );
    assert_eq!(
        status_after(
            &actions,
            "pay-run/00000000-0000-0000-0000-000000000000/pay-run-line/actions/add_line"
        ),
        Some(404),
        "the bogus-parent leg must pin 404:\n{actions}"
    );
    assert!(
        actions.contains("bogus parent answers 404"),
        "the bogus-parent leg must be labelled as the parent-load failure leg:\n{actions}"
    );
    // The read-back proves the element landed and is readable under the parent.
    assert!(
        actions.contains(
            "GET {{base_url}}/api/v1/payroll/pay-run/{{pay_run_id}}/pay-run-line/{{added_add_line_id}}"
        ),
        "the add leg must read the element back by id:\n{actions}"
    );
    assert!(
        actions.contains("jsonpath \"$.data.id\" exists"),
        "the read-back must assert the element exists:\n{actions}"
    );
    // The failure legs: the create DTO's unknown-field denial and the
    // parent-ownership 404.
    assert!(
        actions.contains("__unknown_field__"),
        "the add failure leg must send an unknown field:\n{actions}"
    );
    assert!(
        actions.contains(
            "{{base_url}}/api/v1/payroll/pay-run/{{pay_run_id}}/pay-run-line/00000000-0000-0000-0000-000000000000/actions/remove_line"
        ),
        "the remove failure leg must target an element the parent does not own:\n{actions}"
    );
    assert!(
        actions.contains("HTTP 422") && actions.contains("HTTP 404"),
        "the child action contracts must pin both failure legs:\n{actions}"
    );
    // save/remove ride the item route and answer 200.
    assert_eq!(
        status_after(&actions, "/actions/save_line"),
        Some(200),
        "{actions}"
    );
    assert_eq!(
        status_after(&actions, "/actions/remove_line"),
        Some(200),
        "{actions}"
    );
    // The seeded row must survive long enough for the item legs: remove runs
    // LAST (it deletes the captured element).
    let save_at = actions.find("/actions/save_line").expect("save leg");
    let remove_at = actions.find("/actions/remove_line").expect("remove leg");
    assert!(
        save_at < remove_at,
        "`remove` must run after `save` (it deletes the captured row):\n{actions}"
    );
}

/// Fail-closed pin: the SAME model with the child entity NOT declared a
/// nested child warns and emits no route (never a top-level route that
/// silently drops the parent anchor), and the app still generates.
#[tokio::test]
async fn child_operations_without_a_nested_child_declaration_emit_nothing() {
    let fixture = write_child_ops_fixture_with(
        r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.payroll]
label = "Payroll"
schema_dir = "model-schemas"
postgres_schema = "payroll"
"#,
    );
    let engine = ingest_child_ops(&fixture).await;
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
    let child_router = &router[router
        .find("fn pay_run_line_routes()")
        .expect("the child routes fn still emits")..];
    for snake in ["add_line", "save_line", "remove_line"] {
        assert!(
            !child_router.contains(&format!("/actions/{snake}")),
            "an un-nested child must emit no `{snake}` route:\n{child_router}"
        );
    }
    let handler = read(&out.path().join("src/api/payroll/pay_run_line_handler.rs"));
    assert!(
        !handler.contains("service_op_add_line"),
        "an un-nested child must emit no child action handler:\n{handler}"
    );
}

/// Issue #692: the collection-level `add` proves the parent row exists
/// BEFORE the create — a parent load-or-404 through the parent's own
/// RLS-scoped `find_by_id` query slot, so a bogus or foreign-org
/// `parent_id` answers 404 instead of surfacing Postgres `23503` as a 500
/// (the domain error classifier maps only scope/role/RLS markers).
#[tokio::test]
async fn child_collection_add_loads_the_parent_before_the_create() {
    let fixture = write_child_ops_fixture();
    let engine = ingest_child_ops(&fixture).await;
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

    let handler = read(&out.path().join("src/api/payroll/pay_run_line_handler.rs"));
    let add_at = handler
        .find("fn service_op_add_line")
        .expect("the add handler emits");
    let add_arm = &handler[add_at..];
    // (a) The parent load rides the PARENT's own query slot (same domain,
    // the parent's module stem), carrying the parent's auditable arity,
    // and PRECEDES the child create command.
    let load_needle = "state.payroll_pay_run_queries\n        .find_by_id(parent_id, false, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone())";
    let load_at = add_arm.find(load_needle).unwrap_or_else(|| {
        panic!("the parent load-or-404 must ride the parent's RLS-scoped query slot:\n{add_arm}")
    });
    let create_at = add_arm
        .find("_commands.create(")
        .expect("the child create call");
    assert!(
        load_at < create_at,
        "the parent load must precede the create command (the FK write only runs \
         for a parent the session can see):\n{add_arm}"
    );
    // (b) The load-or-404 shape: a miss (a bogus or foreign-org parent —
    // the query plane is RLS-scoped) answers 404 naming the parent
    // aggregate, never 403, never 500.
    assert!(
        add_arm.contains(r#"AppError::not_found(format!("PayRun {parent_id} not found"))"#),
        "the load-or-404 must answer 404 naming the parent aggregate:\n{add_arm}"
    );
    // The item-level actions keep their scoped child load — no parent
    // reload there (the scoped load IS the parent-ownership contract).
    let save_arm = &handler[handler.find("fn service_op_save_line").expect("save arm")..];
    assert!(
        save_arm.contains("find_by_id_scoped(pay_run_line_id, parent_id")
            && !save_arm.contains("pay_run_queries"),
        "the save arm must keep the scoped child load and gain no parent reload:\n{save_arm}"
    );
}

/// Issue #692, fail closed: a parent whose resolved op set exposes NO
/// by-id load (the design surface narrowed to `findAll` — the query.tera
/// rule `has_create or (has_read and not parent_ref)` answers false) is a
/// NAMED generation error at route planning, both through the
/// pre-generation validation pass and the generation path itself. Never a
/// handler that references a query method that does not exist.
#[tokio::test]
async fn child_add_without_a_parent_by_id_load_fails_closed_naming_the_parent() {
    let fixture = write_child_ops_fixture();
    fs::write(
        &fixture.ddd,
        fixture::child_ops_ddd_without_parent_find_by_id(),
    )
    .unwrap();
    let engine = ingest_child_ops(&fixture).await;
    let config = codegraph_config::config::parse_domain_config(&fixture.config).unwrap();

    // The direct collector (the run path's source).
    let refusals =
        codegraph_generate::ddd::service_ops::child_parent_load_refusals(&engine, &config)
            .await
            .unwrap();
    let refusal = refusals
        .iter()
        .find(|r| r.contains("addLine"))
        .unwrap_or_else(|| panic!("expected a parent-load refusal naming `addLine`: {refusals:?}"));
    assert!(
        refusal.contains("PayRunType"),
        "the refusal must name the parent aggregate: {refusal}"
    );
    assert!(
        refusal.contains("find_by_id"),
        "the refusal must name the missing query: {refusal}"
    );

    // The pre-generation validation pass turns it into an Error issue.
    let issues = codegraph::validate::ValidationPass::run(&engine, &config).await;
    let issue = issues
        .iter()
        .find(|i| {
            matches!(i.severity, codegraph::validate::Severity::Error)
                && i.check == "child_op_parent_load"
        })
        .unwrap_or_else(|| panic!("expected a hard parent-load validation error, got: {issues:?}"));
    assert!(
        issue.message.contains("addLine"),
        "the validation error must name the child op: {issue:?}"
    );

    // The generation path refuses too — the driver fails closed with the
    // named Config error (a tree that would not compile is never written).
    let out = tempdir();
    let domain_types = tempdir();
    let hooks = tempdir();
    let error = run_generation_result(
        &engine,
        &fixture,
        out.path(),
        domain_types.path(),
        hooks.path(),
    )
    .await
    .expect_err("the parent-less parent load must fail generation");
    let rendered = error.to_string();
    assert!(
        rendered.contains("addLine") && rendered.contains("find_by_id"),
        "the generation error must name the child op and the missing query: {rendered}"
    );
}

// ── Issue #666 slice 4b: authored aggregate boundaries ─────────────────

/// The AUTHORED verdict rides the graph: `root` on the root design and
/// `aggregate PayRunType` on the child mirror as `is_root` + an
/// `Authored`-origin membership (slice 1) — the origin field is what the
/// slice 4b consumers key on.
#[tokio::test]
async fn authored_aggregate_boundary_mirrors_with_its_origin() {
    let fixture = write_child_ops_fixture_with_sources(
        fixture::AUTHORED_BOUNDARY_DOMAINS_TOML,
        &fixture::child_ops_ddd_with_authored_boundary(),
    );
    let engine = ingest_child_ops(&fixture).await;
    let models = engine.get_ddd_models().await.unwrap();

    let root = models[0]
        .designs
        .iter()
        .find(|d| d.class == "PayRunType")
        .expect("the root design");
    let root_boundary = root.aggregate_boundary.as_ref().expect("root verdict");
    assert_eq!(root_boundary.is_root, Some(true), "root verdict mirrors");
    assert!(
        root_boundary.membership.is_none(),
        "a root carries no membership"
    );

    let child = models[0]
        .designs
        .iter()
        .find(|d| d.class == "PayRunLineType")
        .expect("the child design");
    let child_boundary = child.aggregate_boundary.as_ref().expect("member verdict");
    assert_eq!(child_boundary.is_root, Some(false));
    let membership = child_boundary
        .membership
        .as_ref()
        .expect("the authored membership");
    assert_eq!(
        membership.origin,
        codegraph_core::types::DddAggregateOrigin::Authored,
        "the authored clause is the origin slice 4b keys on"
    );
    assert_eq!(
        membership.root_json.get("type").and_then(|t| t.as_str()),
        Some("class"),
        "the root rides as the adjacently tagged TypeRef JSON"
    );
    assert_eq!(
        membership
            .root_json
            .pointer("/value/name")
            .and_then(|n| n.as_str()),
        Some("PayRunType"),
        "the authored root class name is the value.name leaf"
    );

    // The unauthored twin: a design without an authored clause never
    // carries an Authored membership (the derived origin is informational).
    let fixture2 = write_child_ops_fixture();
    let engine2 = ingest_child_ops(&fixture2).await;
    let models2 = engine2.get_ddd_models().await.unwrap();
    let line = models2[0]
        .designs
        .iter()
        .find(|d| d.class == "PayRunLineType")
        .expect("the unauthored child design");
    assert!(
        line.aggregate_boundary.as_ref().is_none_or(|b| b
            .membership
            .as_ref()
            .is_none_or(|m| m.origin != codegraph_core::types::DddAggregateOrigin::Authored)),
        "a design without an authored clause never carries an Authored membership"
    );
}

/// (a) The authored membership is the ONLY nesting declaration: with NO
/// config `role = "child"`, the child-op routes still emit (the nesting
/// gate consumes the authored verdict) with the collection-add parent load
/// resolved, the router plane nests the child under the authored root, and
/// the generated tree carries the nested handler surface. The
/// repository-bearing authored member rides the HAND-BUILT graph mirror —
/// upstream Rule 6 forbids a member repository, so the compiler front door
/// cannot produce this shape (see [`fixture::authored_boundary_graph`]);
/// the mirror is the slice-1 contract slice 4b consumes.
#[tokio::test]
async fn authored_boundary_nests_the_child_and_resolves_the_parent_load() {
    let fixture = write_child_ops_fixture_with_sources(
        fixture::AUTHORED_BOUNDARY_DOMAINS_TOML,
        &fixture::child_ops_ddd_with_authored_boundary(),
    );
    let engine = ingest_authored_boundary_graph(&fixture).await;
    let config = codegraph_config::config::parse_domain_config(&fixture.config).unwrap();

    // The service-op plane: the child ops emit with the add's parent load
    // resolved (issue #692 facts ride the same nesting verdict).
    let routes = codegraph_generate::ddd::service_ops::routes_for_entity(
        &engine,
        &config,
        "payroll",
        "PayRunLineType",
        &codegraph_generate::ddd::service_ops::BodyRenderContext::default(),
    )
    .await
    .expect("routes resolve");
    let add = routes
        .iter()
        .find(|r| r.op_name == "addLine")
        .expect("the authored nesting must emit the add route");
    let codegraph_generate::ddd::service_ops::ExecutionKind::ChildOp {
        action,
        parent_load,
        ..
    } = &add.execution
    else {
        panic!("expected a ChildOp execution, got: {:?}", add.execution)
    };
    assert_eq!(
        *action,
        codegraph_generate::ddd::service_ops::ChildAction::Add
    );
    let load = parent_load
        .as_ref()
        .expect("the collection add must resolve the parent load from the authored verdict");
    assert_eq!(load.module, "pay_run", "the parent's query slot resolves");
    assert_eq!(load.domain, "payroll", "same-domain by construction");

    // The router plane nests the child under the authored root with the
    // refers-derived FK column.
    let project = codegraph_generate::ProjectConfig::default();
    let parent_candidates = engine.get_parent_candidates().await.expect("candidates");
    let ctx = codegraph_generate::api::router::build_router_context(
        &engine,
        "payroll",
        &[
            "PayRunType".to_string(),
            "PayRunLineType".to_string(),
            "PayRunLineAdjustmentType".to_string(),
        ],
        &config,
        &parent_candidates,
        &project,
    )
    .await
    .expect("router context");
    let line = ctx
        .entities
        .iter()
        .find(|e| e.module_name == "pay_run_line")
        .expect("the child entity");
    assert_eq!(line.role, "child", "the authored member routes as a child");
    let parent = line.parent.as_ref().expect("authored parent assignment");
    assert_eq!(parent.module_name, "pay_run");
    assert_eq!(parent.fk_column, "pay_run_id");
    let root = ctx
        .entities
        .iter()
        .find(|e| e.module_name == "pay_run")
        .expect("the root entity");
    assert_eq!(root.role, "root", "the authored root stays top-level");
    assert!(
        root.children
            .iter()
            .any(|c| c.module_name == "pay_run_line"),
        "the parent gains the authored child"
    );

    // The generated tree carries the nested surface: the handler extracts
    // the two-uuid path and loads the record through the parent-scoped
    // read; the router nests the child fn under the parent.
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
    let root_router = &router[router.find("fn pay_run_routes()").expect("root routes fn")..];
    assert!(
        root_router.contains(".nest(\"/{pay_run_id}/pay-run-line\", pay_run_line_routes())"),
        "the authored child must nest under the parent:\n{root_router}"
    );
    let handler = read(&out.path().join("src/api/payroll/pay_run_line_handler.rs"));
    assert!(
        handler.contains("Path((parent_id, pay_run_line_id)): Path<(Uuid, Uuid)>"),
        "the authored child handler extracts the parent anchor:\n{handler}"
    );
    assert!(
        handler.contains("find_by_id_scoped(pay_run_line_id, parent_id"),
        "the authored child's query plane carries the parent-scoped read:\n{handler}"
    );
    let add_at = handler
        .find("fn service_op_add_line")
        .expect("the add handler emits");
    let add_arm = &handler[add_at..];
    let load_at = add_arm
        .find("state.payroll_pay_run_queries")
        .expect("the collection add still loads the parent first (issue #692)");
    let create_at = add_arm.find("_commands.create(").expect("the child create");
    assert!(
        load_at < create_at,
        "the parent load must precede the create under the authored nesting:\n{add_arm}"
    );
}

/// (iii) Authored member vs config parent disagreement: the design authors
/// `aggregate PayRunType` while config declares
/// `parent = "PayRunLineAdjustmentType"`. The WARN records the override
/// (naming the class, the authored root, and config's parent), and the
/// routing still nests under the AUTHORED root.
#[tokio::test]
async fn authored_member_overrides_a_config_parent_disagreement_with_a_warning() {
    let fixture = write_child_ops_fixture_with_sources(
        fixture::AUTHORED_BOUNDARY_MISMATCH_DOMAINS_TOML,
        &fixture::child_ops_ddd_with_authored_boundary(),
    );
    let engine = ingest_child_ops(&fixture).await;
    let config = codegraph_config::config::parse_domain_config(&fixture.config).unwrap();

    let issues = codegraph::validate::ValidationPass::run(&engine, &config).await;
    let issue = issues
        .iter()
        .find(|i| {
            matches!(i.severity, codegraph::validate::Severity::Warning)
                && i.check == "aggregate_boundary"
        })
        .unwrap_or_else(|| panic!("expected an aggregate-boundary WARN, got: {issues:?}"));
    assert!(
        issue.message.contains("PayRunLineType"),
        "the WARN names the class: {issue:?}"
    );
    assert!(
        issue.message.contains("PayRunType"),
        "the WARN names the authored root: {issue:?}"
    );
    assert!(
        issue.message.contains("PayRunLineAdjustmentType"),
        "the WARN names config's overridden parent: {issue:?}"
    );

    // Authored wins for routing: the child nests under the pay run.
    let project = codegraph_generate::ProjectConfig::default();
    let parent_candidates = engine.get_parent_candidates().await.expect("candidates");
    let ctx = codegraph_generate::api::router::build_router_context(
        &engine,
        "payroll",
        &[
            "PayRunType".to_string(),
            "PayRunLineType".to_string(),
            "PayRunLineAdjustmentType".to_string(),
        ],
        &config,
        &parent_candidates,
        &project,
    )
    .await
    .expect("router context");
    let line = ctx
        .entities
        .iter()
        .find(|e| e.module_name == "pay_run_line")
        .expect("the child entity");
    let parent = line.parent.as_ref().expect("authored parent assignment");
    assert_eq!(
        parent.module_name, "pay_run",
        "config's parent is overridden by the authored verdict"
    );
}

/// (ii) Authored root vs config child: the member's authored declaration
/// proves `PayRunType` an aggregate root while config declares IT a child.
/// The WARN names both facts, and the routing keeps the pay run a
/// top-level root (the derived-only shape never warns).
#[tokio::test]
async fn authored_root_beats_a_config_child_declaration_with_a_warning() {
    let fixture = write_child_ops_fixture_with_sources(
        fixture::AUTHORED_ROOT_VS_CONFIG_CHILD_DOMAINS_TOML,
        &fixture::child_ops_ddd_with_authored_boundary(),
    );
    let engine = ingest_child_ops(&fixture).await;
    let config = codegraph_config::config::parse_domain_config(&fixture.config).unwrap();

    let issues = codegraph::validate::ValidationPass::run(&engine, &config).await;
    let issue = issues
        .iter()
        .find(|i| {
            matches!(i.severity, codegraph::validate::Severity::Warning)
                && i.check == "aggregate_boundary"
                && i.message.contains("PayRunType")
        })
        .unwrap_or_else(|| panic!("expected an authored-root WARN, got: {issues:?}"));
    assert!(
        issue.message.contains("root"),
        "the WARN names the authored root verdict: {issue:?}"
    );
    assert!(
        issue.message.contains("PayRunLineAdjustmentType"),
        "the WARN names config's child declaration: {issue:?}"
    );

    // Authored wins: the pay run stays a top-level root even though config
    // declares it a child.
    let project = codegraph_generate::ProjectConfig::default();
    let parent_candidates = engine.get_parent_candidates().await.expect("candidates");
    let ctx = codegraph_generate::api::router::build_router_context(
        &engine,
        "payroll",
        &[
            "PayRunType".to_string(),
            "PayRunLineType".to_string(),
            "PayRunLineAdjustmentType".to_string(),
        ],
        &config,
        &parent_candidates,
        &project,
    )
    .await
    .expect("router context");
    let root = ctx
        .entities
        .iter()
        .find(|e| e.module_name == "pay_run")
        .expect("the root entity");
    assert_eq!(
        root.role, "root",
        "the authored root verdict overrides the config child role"
    );
    assert!(root.parent.is_none());
}

// ── Declared error vocabulary + `throws` (DD7, issue #665) ──────────────

/// The `.ddd` declared error vocabulary generates as `{Domain}Error`
/// variants (payload fields typed through the shared derivation), and a
/// delegation op's `throws` documents its declared statuses on the action
/// path. `NotFound` adds no utoipa entry (the base set documents 404) but
/// DOES get a deterministic hurl 404 leg.
#[tokio::test]
async fn declared_error_vocabulary_generates_and_documents_on_delegations() {
    let fixture = write_fixture_in("service_ops_throws", PAYROLL_DDD);
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

    // ── errors.rs: the declared vocabulary, both scopes ──
    let errors = read(&out.path().join("src/domain/payroll/errors.rs"));
    // Application-scope error with a payload: codelist field + primitive
    // field, named-field variant, `{ .. }` match arms.
    assert!(
        errors.contains("PaymentDeclined {"),
        "the payload variant must generate:\n{errors}"
    );
    assert!(
        errors.contains("reason: crate::codelist::DeclineReason,"),
        "the codelist payload field must resolve to the generated enum:\n{errors}"
    );
    assert!(
        errors.contains("reference: String,"),
        "the primitive payload field must map:\n{errors}"
    );
    assert!(
        errors.contains("Self::PaymentDeclined { .. } => \"PAYMENT_DECLINED\""),
        "the code() arm must carry the payload pattern:\n{errors}"
    );
    // The payload-less application error stays a unit variant; the
    // module-private error generates into its own domain's enum.
    assert!(errors.contains("PayrollFrozen,"), "{errors}");
    assert!(errors.contains("LineLocked {"), "{errors}");
    // Prelude cannot be shadowed: no declared variant redefines it.
    assert!(!errors.contains("NotFound(P"), "{errors}");

    // ── handler utoipa: the declared status on the delegation surface ──
    let handler = read(&out.path().join("src/api/payroll/pay_run_handler.rs"));
    assert!(
        handler.contains(
            "(status = 422, description = \"The operation raised PaymentDeclined (declared `throws`)\")"
        ),
        "the declared 422 must document on the declines action:\n{handler}"
    );
    // 404 is already in the base set — no duplicate entry for NotFound.
    assert!(
        !handler.contains("The operation raised NotFound"),
        "a base-set-covered status must not duplicate:\n{handler}"
    );

    // ── hurl: the deterministic declared-error leg ──
    let actions = read(&out.path().join("hurl/12_payroll_pay_run_actions.hurl"));
    assert!(
        actions.contains("declared `throws NotFound`"),
        "the DD7 note must label the leg:\n{actions}"
    );
    assert!(
        actions.contains("/00000000-0000-0000-0000-000000000000/actions/declines"),
        "the zero-uuid action path must pin the 404:\n{actions}"
    );
}

/// A `throws` entry naming no prelude error and no declared variant is a
/// NAMED refusal naming the op and the entry. The pherolang compiler
/// rejects an unknown name upstream (E0373), so this pins the graph-level
/// gate for the stale/hand-built artifact class: a persisted model naming
/// a declaration that no longer resolves (a warm cache whose vocabulary
/// moved, a hand-assembled graph).
#[tokio::test]
async fn unresolvable_throws_entry_fails_validation_naming_the_op() {
    use codegraph_core::traits::GraphIngestor;

    let fixture = write_fixture_in("service_ops_throws_refusal", PAYROLL_DDD);
    let config = codegraph_config::config::parse_domain_config(&fixture.config).unwrap();

    // A hand-built model: the compiler can never produce this shape, which
    // is exactly the point — the gate catches what outlives the compile.
    let graph = codegraph_core::types::DddModelGraph {
        source_path: "payroll.ddd".to_string(),
        application: codegraph_core::types::DddApplicationNode {
            name: "Payroll".to_string(),
            base: None,
            source_path: "payroll.ddd".to_string(),
            errors: Vec::new(),
        },
        modules: vec![codegraph_core::types::DddModuleNode {
            application: "Payroll".to_string(),
            name: "payroll".to_string(),
            ordinal: 0,
            errors: Vec::new(),
        }],
        designs: Vec::new(),
        repositories: vec![codegraph_core::types::DddRepositoryNode {
            application: "Payroll".to_string(),
            name: "PayRunRepository".to_string(),
            design_class: "PayRunType".to_string(),
            operations: Vec::new(),
        }],
        services: vec![codegraph_core::types::DddServiceNode {
            application: "Payroll".to_string(),
            module: "payroll".to_string(),
            name: "PayRunService".to_string(),
            description: None,
            dependencies: vec!["PayRunRepository".to_string()],
            decision_dependencies: Vec::new(),
            operations: vec![codegraph_core::types::DddServiceOperation {
                name: "declines".to_string(),
                return_type: None,
                return_multiplicity: None,
                params: Vec::new(),
                delegation_target: Some("PayRunRepository".to_string()),
                delegation_operation: Some("save".to_string()),
                capabilities: Vec::new(),
                is_protected: false,
                condition: None,
                emits: Vec::new(),
                invokes: Vec::new(),
                throws: vec![serde_json::json!({
                    "name": "Bogus",
                    "scope": "module",
                    "module": "payroll"
                })],
                bodies: Default::default(),
                body_receiver: None,
                transition: None,
                child_scope: None,
                condition_paths: Vec::new(),
                ordinal: 0,
            }],
            ordinal: 0,
        }],
        searches: Vec::new(),
    };
    let engine = codegraph_core::mock::MockEngine::builder()
        .with_schema(fixture::pay_run_schema_node())
        .build();
    engine.ingest_ddd_model(&graph).await.unwrap();

    let refusals = codegraph_generate::ddd::service_ops::throws_refusals(&engine, &config)
        .await
        .unwrap();
    let refusal = refusals
        .iter()
        .find(|r| r.contains("declines"))
        .unwrap_or_else(|| panic!("expected a throws refusal naming the op: {refusals:?}"));
    assert!(
        refusal.contains("`Bogus`"),
        "the refusal must name the entry: {refusal}"
    );
    assert!(
        refusal.contains("no generated error"),
        "the refusal must name the resolution failure: {refusal}"
    );
}

/// A domains.toml override keyed by the declared error name keeps its
/// description/status, adopts the model's payload fields, and moves the
/// documented status.
#[tokio::test]
async fn domain_error_override_keeps_status_and_adopts_fields() {
    let fixture = write_fixture_in("service_ops_throws_override", PAYROLL_DDD);
    let config_source = format!(
        "{DOMAINS_TOML}\n[domains.payroll.errors]\nPaymentDeclined = {{ description = \"declined by the processor\", http_status = 409 }}\n"
    );
    fs::write(&fixture.config, config_source).unwrap();
    let engine = ingest(&fixture).await;
    // On the run path `ingest_api_model` seeds this node from the config's
    // `[domains.payroll.errors]` walk; the test seeds it directly to pin
    // the MERGE contract (the node is the derivation's only input).
    use codegraph_core::traits::GraphIngestor;
    engine
        .ingest_error_definition(&codegraph_core::types::ErrorDefinitionNode {
            code: "PaymentDeclined".to_string(),
            description: "declined by the processor".to_string(),
            http_status: 409,
            domain: Some("payroll".to_string()),
        })
        .await
        .unwrap();
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

    let errors = read(&out.path().join("src/domain/payroll/errors.rs"));
    // The override's status wins; the model's payload fields stay.
    assert!(
        errors.contains("Self::PaymentDeclined { .. } => StatusCode::from_u16(409)"),
        "the override status must win:\n{errors}"
    );
    assert!(
        errors.contains("reason: crate::codelist::DeclineReason,"),
        "the override must not drop the payload fields:\n{errors}"
    );
    let handler = read(&out.path().join("src/api/payroll/pay_run_handler.rs"));
    assert!(
        handler.contains(
            "(status = 409, description = \"The operation raised PaymentDeclined (declared `throws`)\")"
        ),
        "the documented status must follow the override:\n{handler}"
    );
}
