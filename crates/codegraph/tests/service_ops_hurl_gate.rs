//! Live acceptance gate for the #590 service-ops plane: the emitted
//! `hurl/10_payroll_pay_run_actions.hurl` action contract must SERVE —
//! real HTTP against the generated axum app over a scratch Postgres —
//! proving the whole lowering chain: 201 create / 200 wired delegations /
//! 409 wrong-state transition guards / 501 abstract-op seams.
//!
//! Pipeline: generate the pay_run fixture app (the `service_ops_compile_gate`
//! recipe: path deps, gRPC stripped) → scratch DB → migrations → API key →
//! boot the generated server → hurl. Plus a HAND-AUTHORED wrong-state probe
//! on its own row (create → submit 200 → submit replay 403: the op-level
//! `when` transition guard is business semantics applying to every session
//! → approve 200 → approve replay 409: the machine guard, unconditional
//! grant → notify 501 → unknown-id read 404) and a DB-level proof that the
//! submitted row really advanced to `Calculating`. Capability/role policy
//! gates have NO legs here (API-key sessions bypass them); the emitted
//! contract pins no 403 either.
//!
//! The gate parses the CREATE entry and the action-route base out of the
//! EMITTED contract rather than hardcoding them, so wire-key casing or
//! route-segment changes cannot desync the hand-authored legs.
//!
//! Self-contained: skips gracefully without hurl or Postgres (the
//! `rules_guard_api_gate` precedent). Node-free unit tests pin the skip
//! logic and the assertion/extraction surface.
//!
//! ```text
//! cargo test -p codegraph --test service_ops_hurl_gate -- --ignored --nocapture
//! ```

#[path = "service_ops_fixture.rs"]
mod fixture;

#[path = "test_framework/mod.rs"]
mod test_framework;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use test_framework::axum_server;
use test_framework::postgres::{GateDb, database_reachable, resolve_base_target};
use test_framework::process::have_tool;

/// The emitted per-entity actions contract for the fixture's one entity
/// (`ENTITY_NN_BASE = 10`, one entity → `10_payroll_pay_run_actions.hurl`;
/// pinned by `service_ops_tests.rs`).
fn actions_path(output: &Path) -> PathBuf {
    output.join("hurl/10_payroll_pay_run_actions.hurl")
}

fn gate_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/service-ops-gate")
}

/// The generated app lives in the gate root (NOT a tempdir) so the
/// app-side cargo target cache survives across runs.
fn gate_app_dir() -> PathBuf {
    gate_root().join("app")
}

/// Isolated `CARGO_TARGET_DIR` for the generated-app build — sharing the
/// workspace target dir would deadlock on the workspace cargo lock.
fn gate_target_dir() -> PathBuf {
    gate_root().join("target")
}

fn logs_dir() -> PathBuf {
    gate_root().join("logs")
}

fn skip(msg: &str) {
    println!("skipping: {msg}");
}

/// The minimal `.ifml` shim the gate feeds the driver: policies reach the
/// driver's graph ONLY through `.ifml` `import` statements (Pass 1b) — a
/// `.ddd`-only run leaves the actor policy un-ingested, capability-only ops
/// bind no entity (`bind_entity` step 1 reads `get_capabilities()`), and
/// the `notify` seam emits no route (the dead-surface WARN) — no 501 leg.
/// No IFML framework targets are declared, so NO IFML UI artifacts emit;
/// the shim only carries the policy into the graph.
const POLICY_SHIM_IFML: &str = r#"import "policy.actor";

domain "payroll" { schema "payroll"; }
"#;

/// The gate's cheap prerequisites: hurl runs the contracts over real HTTP.
/// Postgres reachability is the caller's check (async, needs a target).
/// Split from the live test so the node-free unit test can pin the skip
/// logic without mutating PATH.
fn gate_prerequisites(hurl_available: bool) -> Result<(), String> {
    if !hurl_available {
        return Err(
            "hurl is not installed — the service-ops gate runs the emitted \
                    hurl contracts over real HTTP (install: cargo install --locked hurl, \
                    or a GitHub release binary)"
                .to_string(),
        );
    }
    Ok(())
}

/// The fixture profiles: the workspace default with path deps (the gate
/// must build without network) and the gRPC generators stripped (no
/// protoc requirement). Same recipe as `service_ops_compile_gate`, plus
/// `type_contracts_base`: without it the generated domain-types crate pins
/// `codegraph-type-contracts` at this checkout's (unpushed) git rev and
/// the build fetch fails.
fn write_profiles(dir: &Path) -> std::path::PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    let workspace_profiles = workspace_root.join("profiles.toml");
    let type_contracts = workspace_root.join("crates/codegraph-type-contracts");
    let mut text = fs::read_to_string(&workspace_profiles).unwrap();
    text = text.replace(
        "[profiles.default.meta]\n",
        &format!(
            "[profiles.default.meta]\n\
             domain_types_base = \"crates/domain-types\"\n\
             type_contracts_base = \"{}\"\n",
            type_contracts.display()
        ),
    );
    text = text.replace(
        "[profiles.default.features]\n",
        "[profiles.default.features]\ndependency_strategy = \"path\"\n",
    );
    let filtered: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim_start().starts_with("\"grpc_"))
        .collect();
    let path = dir.join("profiles.toml");
    fs::write(&path, filtered.join("\n")).unwrap();
    path
}

/// Run hurl over the given files against the booted server (the
/// `rules_guard_api_gate` recipe, verbatim).
fn run_hurl(port: u16, api_key: &str, files: &[PathBuf]) -> Result<(), String> {
    let mut cmd = Command::new("hurl");
    cmd.arg("--test")
        .arg("--error-format")
        .arg("long")
        .arg("--variable")
        .arg(format!("base_url=http://127.0.0.1:{port}"))
        .arg("--variable")
        .arg(format!("api_key={api_key}"))
        .args(files);
    let out = cmd
        .output()
        .map_err(|e| format!("failed to spawn hurl: {e}"))?;
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if !out.status.success() {
        let tail: String = combined
            .lines()
            .rev()
            .take(60)
            .collect::<Vec<_>>()
            .join("\n");
        return Err(format!("hurl failed ({})\n{tail}", out.status));
    }
    println!("hurl: {}", combined.lines().last().unwrap_or(""));
    Ok(())
}

/// A no-op pgmq stand-in for plain Postgres targets (the
/// `rules_guard_api_gate` precedent): queue create/send become sinks.
/// Staged over the generated pgmq setup migration when the target cluster
/// lacks the pgmq extension.
const PGMQ_SHIM: &str = "\
-- Test shim: pgmq stand-in (no extension available).
CREATE SCHEMA IF NOT EXISTS pgmq;
CREATE OR REPLACE FUNCTION pgmq.create(p_name text) RETURNS void
AS $shim$ BEGIN END $shim$ LANGUAGE plpgsql;
CREATE OR REPLACE FUNCTION pgmq.send(p_queue text, p_msg jsonb) RETURNS bigint
AS $shim$ SELECT 1 $shim$ LANGUAGE sql;
";

/// The generated migrations, with the pgmq setup replaced by the shim when
/// the target cluster has no pgmq extension (copied as a PAIR with
/// `PGMQ_SHIM` from `rules_guard_api_gate`).
async fn resolve_migrations_dir(
    target: &codegraph_ops::pg::PgTarget,
    app_dir: &Path,
) -> (PathBuf, Option<tempfile::TempDir>) {
    let migrations = app_dir.join("migrations");
    let has_pgmq = codegraph_ops::db::psql_query(
        target,
        "SELECT count(*) FROM pg_available_extensions WHERE name = 'pgmq';",
    )
    .await
    .map(|v| v == "1")
    .unwrap_or(false);
    if has_pgmq {
        return (migrations, None);
    }
    let staged = tempfile::tempdir().expect("stage migrations tempdir");
    let dst = staged.path().join("migrations");
    fs::create_dir_all(&dst).expect("create staged migrations dir");
    for entry in fs::read_dir(&migrations).expect("read generated migrations") {
        let entry = entry.expect("read migration entry");
        let name = entry.file_name();
        let dst_path = dst.join(&name);
        if name.to_string_lossy().contains("pgmq_setup") {
            fs::write(&dst_path, PGMQ_SHIM).expect("write pgmq shim");
        } else {
            fs::copy(entry.path(), &dst_path).expect("copy migration file");
        }
    }
    (dst, Some(staged))
}

// ── Emitted-surface assertions (cheap, pre-build) ──────────────────────

/// The emitted actions contract must carry every live leg class BEFORE any
/// expensive build spend: 201 create, wired 200 legs, the 409 wrong-state
/// guard, the 501 seam naming its gap, and NO 403 pin (API-key sessions
/// bypass the policy gates by design).
fn assert_emitted_surface(output: &Path) -> Result<(), String> {
    let path = actions_path(output);
    let actions = fs::read_to_string(&path).map_err(|e| {
        format!(
            "the emitted actions contract is missing ({}): {e}",
            path.display()
        )
    })?;
    let required = [
        ("HTTP 201", "the create leg"),
        ("/actions/submit", "the machine-bound submit route"),
        ("HTTP 200", "a wired 200 delegation leg"),
        ("HTTP 409", "the wrong-state guard leg"),
        ("HTTP 501", "the abstract-op seam leg"),
        ("has no delegation target", "the seam reason naming the gap"),
        ("wrong-state", "the wrong-state leg documentation"),
    ];
    for (needle, why) in required {
        if !actions.contains(needle) {
            return Err(format!(
                "the emitted actions contract lacks {why} (`{needle}`):\n{actions}"
            ));
        }
    }
    if actions.contains("HTTP 403") {
        return Err(
            "the actions contract pins a 403 leg — API-key sessions bypass the \
             policy gates by design; 403s ride JWT sessions"
                .to_string(),
        );
    }
    Ok(())
}

/// The actions contract's CREATE entry, parsed out of the emitted file:
/// `(post_url, create_body_lines)`. The `# CREATE` comment anchor skips any
/// required-FK preamble POSTs above it.
fn extract_create_entry(actions: &str) -> Result<(String, String), String> {
    let anchor = actions
        .lines()
        .position(|l| l.starts_with("# CREATE"))
        .ok_or("the actions contract has no `# CREATE` anchor")?;
    let mut url: Option<String> = None;
    let mut body_lines: Vec<&str> = Vec::new();
    let mut in_body = false;
    for line in actions.lines().skip(anchor + 1) {
        if let Some(rest) = line.strip_prefix("POST ") {
            if url.is_some() {
                break; // a second POST means the CREATE entry ended malformed
            }
            url = Some(rest.trim().to_string());
            continue;
        }
        if url.is_some() {
            if line.starts_with('{') {
                in_body = true;
            }
            if in_body {
                body_lines.push(line);
                if line.trim_end() == "}" {
                    break;
                }
            }
        }
    }
    let url = url.ok_or("the actions contract's CREATE entry has no POST line")?;
    if body_lines.is_empty() {
        return Err("the actions contract's CREATE entry has no JSON body".to_string());
    }
    let body = body_lines
        .iter()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n");
    Ok((url, body))
}

/// The action-route base + hurl capture reference, parsed from the first
/// action leg (`POST <base>/{{capture}}/actions/<op>`).
fn extract_action_base(actions: &str) -> Result<(String, String), String> {
    let line = actions
        .lines()
        .find(|l| l.starts_with("POST ") && l.contains("/actions/"))
        .ok_or("the actions contract has no action POST legs")?;
    let url = line
        .strip_prefix("POST ")
        .and_then(|u| u.split_whitespace().next())
        .ok_or("malformed action leg line in the actions contract")?;
    let idx = url
        .find("/actions/")
        .ok_or("the action leg URL lacks /actions/")?;
    let (base, capture) = url[..idx]
        .rsplit_once('/')
        .ok_or("the action leg URL has no capture segment")?;
    if !(capture.starts_with("{{") && capture.ends_with("}}")) {
        return Err(format!(
            "unexpected capture ref `{capture}` in the action leg"
        ));
    }
    Ok((base.to_string(), capture.to_string()))
}

/// Stamp the probe's create body so the DB-level state proof targets THIS
/// file's row alone (the emitted contract creates its own fixture row from
/// the same body).
fn stamp_probe_row(create_body: &str) -> Result<String, String> {
    const SEED: &str = "contract-pay_run";
    const MARKER: &str = "ws1-gate-pay-run";
    if !create_body.contains(SEED) {
        return Err(format!(
            "the emitted create body carries no `{SEED}` fixture value to stamp \
             with the probe marker: {create_body}"
        ));
    }
    Ok(create_body.replace(SEED, MARKER))
}

/// The hand-authored wrong-state probe: own row, driving the machine past
/// its first transition and pinning BOTH denial classes the plane can
/// answer with. Leg plan (single row):
/// create 201 → submit 200 (Draft→Calculating) → submit replay **403**
/// (the op-level `when (payRunStatus == Draft)` transition guard is
/// business semantics — it applies to EVERY session, API keys included;
/// only capability/role gates are bypassed) → approve 200
/// (Calculating→Approved) → approve replay **409** (no machine transition
/// from Approved — approve's grant is unconditional, so the machine guard
/// answers) → notify 501 (abstract seam) → unknown-id read 404.
const WRONG_STATE_TEMPLATE: &str = r#"# WS1 service-ops wrong-state probe — hand-authored, own row (issue #590).
# The emitted contract simulates the machine from `initial` for its own
# row; this probe drives ONE row past its first transition and proves the
# 403 grant-`when` denial, the 409 machine guard, the 501 seam, and the
# 404 contract over real HTTP.
POST @CREATE_URL@
Authorization: Bearer {{api_key}}
@CREATE_BODY@

HTTP 201
[Captures]
@CAPTURE_NAME@: jsonpath "$.data.id"

# wired machine transition: Draft → Calculating
POST @ITEM_BASE@/@CAPTURE@/actions/submit
Authorization: Bearer {{api_key}}

HTTP 200

# op-level `when` transition guard: `submit` requires Draft, the row sits
# in Calculating — business semantics answer 403 to EVERY session
POST @ITEM_BASE@/@CAPTURE@/actions/submit
Authorization: Bearer {{api_key}}

HTTP 403

# second wired transition: Calculating → Approved (unconditional grant)
POST @ITEM_BASE@/@CAPTURE@/actions/approve
Authorization: Bearer {{api_key}}

HTTP 200

# machine guard: `approve` has no transition from Approved — the grant is
# unconditional, so the 409 conflict answers (the wrong-state contract)
POST @ITEM_BASE@/@CAPTURE@/actions/approve
Authorization: Bearer {{api_key}}

HTTP 409

# abstract op — the seam answers 501 naming the gap
POST @ITEM_BASE@/@CAPTURE@/actions/notify
Authorization: Bearer {{api_key}}

HTTP 501

# unknown id — the record load is the route's 404 contract
GET @ITEM_BASE@/00000000-0000-0000-0000-0000000000f0
Authorization: Bearer {{api_key}}

HTTP 404
"#;

fn wrong_state_probe(
    create_url: &str,
    item_base: &str,
    capture: &str,
    capture_name: &str,
    create_body: &str,
) -> String {
    WRONG_STATE_TEMPLATE
        .replace("@CREATE_URL@", create_url)
        .replace("@ITEM_BASE@", item_base)
        .replace("@CAPTURE@", capture)
        .replace("@CAPTURE_NAME@", capture_name)
        .replace("@CREATE_BODY@", create_body)
}

/// A synthetic actions contract in the exact shape the `actions.tera`
/// template emits (anchor, CREATE entry, wired/wrong-state/seam/replay
/// legs, delete delegation last) — the unit-test fixture for the surface
/// assertions and the parsers.
const SYNTHETIC_CONTRACT: &str = r#"# hurl_contract: payroll.pay_run — service-op action contracts (issue #590).
# Regenerate with the codegraph pipeline. Vars: base_url, api_key.
#
# API-key sessions bypass the policy gates (RLS scopes still apply).
# State awareness: a machine-bound op pins 200 only when its transition is
# valid from the record's CURRENT state.
# CREATE — capture the row the actions run against
POST {{base_url}}/api/v1/payroll/pay-run
Authorization: Bearer {{api_key}}
{
  "payRunId": "contract-pay_run",
  "payRunStatus": "Draft"
}

HTTP 201
[Captures]
pay_run_id: jsonpath "$.data.id"
[Asserts]
jsonpath "$.data.id" exists
# service op `Inspect` (service `PayRunService`)
POST {{base_url}}/api/v1/payroll/pay-run/{{pay_run_id}}/actions/inspect
Authorization: Bearer {{api_key}}

HTTP 200
# service op `Archive` (service `PayRunService`)
POST {{base_url}}/api/v1/payroll/pay-run/{{pay_run_id}}/actions/archive
Authorization: Bearer {{api_key}}

HTTP 200
# service op `Reverse` (service `PayRunService`) — wrong-state guard (409): no transition from the current state `Draft`
POST {{base_url}}/api/v1/payroll/pay-run/{{pay_run_id}}/actions/reverse
Authorization: Bearer {{api_key}}

HTTP 409
# service op `Submit` (service `PayRunService`)
POST {{base_url}}/api/v1/payroll/pay-run/{{pay_run_id}}/actions/submit
Authorization: Bearer {{api_key}}

HTTP 200
# service op `Approve` (service `PayRunService`)
POST {{base_url}}/api/v1/payroll/pay-run/{{pay_run_id}}/actions/approve
Authorization: Bearer {{api_key}}

HTTP 200
# service op `Notify` (service `PayRunService`) — 501 seam: `Notify` has no delegation target
POST {{base_url}}/api/v1/payroll/pay-run/{{pay_run_id}}/actions/notify
Authorization: Bearer {{api_key}}

HTTP 501
# service op `Approve` (service `PayRunService`) — wrong-state replay: `Calculating` has no transition from `Approved`
POST {{base_url}}/api/v1/payroll/pay-run/{{pay_run_id}}/actions/approve
Authorization: Bearer {{api_key}}

HTTP 409
# service op `Purge` (service `PayRunService`)
POST {{base_url}}/api/v1/payroll/pay-run/{{pay_run_id}}/actions/purge
Authorization: Bearer {{api_key}}

HTTP 200
"#;

// ── Node-free unit tests (PR-CI) ───────────────────────────────────────

#[test]
fn gate_prerequisites_names_the_missing_tool() {
    let err = gate_prerequisites(false).expect_err("a missing hurl must be reported");
    assert!(err.contains("hurl"), "the error must name the tool: {err}");
    gate_prerequisites(true).expect("present prerequisites must pass");
}

#[test]
fn emitted_surface_assertions_accept_the_real_shape_and_reject_a_gutted_contract() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path();
    fs::create_dir_all(out.join("hurl")).expect("hurl dir");
    let path = actions_path(out);
    fs::write(&path, SYNTHETIC_CONTRACT).expect("write contract");
    assert_emitted_surface(out).expect("the real contract shape must pass the surface check");

    let gutted = SYNTHETIC_CONTRACT
        .lines()
        .filter(|l| !l.contains("HTTP 409") && !l.contains("wrong-state"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&path, gutted).expect("write gutted contract");
    let err =
        assert_emitted_surface(out).expect_err("a gutted contract must fail the surface check");
    assert!(
        err.contains("409"),
        "the error must name the missing leg: {err}"
    );

    fs::write(&path, "HTTP 403\n").expect("write 403 contract");
    let err = assert_emitted_surface(out).expect_err("a 403 pin must fail the surface check");
    assert!(
        err.contains("403"),
        "the error must name the violation: {err}"
    );
}

#[test]
fn extraction_reads_the_create_entry_and_action_base() {
    let (url, body) = extract_create_entry(SYNTHETIC_CONTRACT).expect("create entry");
    assert_eq!(url, "{{base_url}}/api/v1/payroll/pay-run");
    assert_eq!(
        body,
        "{\n  \"payRunId\": \"contract-pay_run\",\n  \"payRunStatus\": \"Draft\"\n}"
    );
    let (base, capture) = extract_action_base(SYNTHETIC_CONTRACT).expect("action base");
    assert_eq!(base, "{{base_url}}/api/v1/payroll/pay-run");
    assert_eq!(capture, "{{pay_run_id}}");

    // Malformed contracts error instead of panicking (typed boundaries).
    assert!(extract_create_entry("no anchor").is_err());
    assert!(extract_action_base("POST {{base_url}}/api/v1/x/1/actions/submit").is_err());
}

#[test]
fn probe_row_stamp_targets_the_hand_authored_row() {
    let stamped = stamp_probe_row("{\n  \"payRunId\": \"contract-pay_run\"\n}").expect("stamp");
    assert!(stamped.contains("ws1-gate-pay-run"), "{stamped}");
    assert!(stamp_probe_row("{\"other\": 1}").is_err());
}

#[test]
fn wrong_state_probe_pins_the_live_legs() {
    let probe = wrong_state_probe(
        "{{base_url}}/api/v1/payroll/pay-run",
        "{{base_url}}/api/v1/payroll/pay-run",
        "{{pay_run_id}}",
        "pay_run_id",
        "{\n  \"pay_run_id\": \"ws1-gate-pay-run\"\n}",
    );
    assert_eq!(probe.matches("HTTP 201").count(), 1, "{probe}");
    assert_eq!(probe.matches("HTTP 200").count(), 2, "{probe}");
    assert_eq!(probe.matches("HTTP 403").count(), 1, "{probe}");
    assert_eq!(probe.matches("HTTP 409").count(), 1, "{probe}");
    assert_eq!(probe.matches("HTTP 501").count(), 1, "{probe}");
    assert_eq!(probe.matches("HTTP 404").count(), 1, "{probe}");
    // The FIRST submit leg pins 200; the replay pins the grant-`when` 403
    // (the op-level transition guard applies to every session).
    let wired = probe
        .find("/actions/submit")
        .expect("the wired submit leg must exist");
    let replay_offset = wired + "/actions/submit".len();
    let replay = probe[replay_offset..]
        .find("/actions/submit")
        .expect("the replay submit leg must exist")
        + replay_offset;
    assert!(probe[wired..replay].contains("HTTP 200"), "{probe}");
    assert!(probe[replay..].contains("HTTP 403"), "{probe}");
    // The approve replay is the machine-guard 409 leg (unconditional grant
    // → the conflict answers, not the policy).
    let approve_replay = probe.rfind("/actions/approve").expect("approve legs");
    assert!(probe[approve_replay..].contains("HTTP 409"), "{probe}");
    // The notify seam and the unknown-id read ride the same capture.
    assert!(probe.contains("{{pay_run_id}}/actions/notify"), "{probe}");
}

// ── The live gate ──────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "live gate: needs Postgres, hurl, and a cargo build of the generated app"]
async fn service_ops_contract_serves_over_http() {
    if let Err(reason) = gate_prerequisites(have_tool("hurl")) {
        skip(&reason);
        return;
    }
    let Ok(base) = resolve_base_target().await else {
        skip("no usable postgres (DATABASE_URL + docker fallback failed)");
        return;
    };
    if !database_reachable(&base).await {
        skip("postgres not reachable");
        return;
    }

    // 1. Generate the fixture app (the compile-gate recipe). The output is
    //    persistent so the app-side cargo cache survives across runs.
    println!("generating the service-ops fixture app…");
    let fixture = fixture::write_fixture_in("service_ops_hurl_gate", fixture::PAYROLL_DDD);
    let _engine = fixture::ingest(&fixture).await;

    let output = gate_app_dir();
    fs::create_dir_all(&output).expect("create gate app dir");
    let fixture_root = fixture.config.parent().expect("fixture root").to_path_buf();
    let profiles_path = write_profiles(&fixture_root);

    // The policy shim rides the fixture root so its `policy.actor` import
    // resolves (see POLICY_SHIM_IFML).
    let ifml_path = fixture_root.join("ws1_gate_policy.ifml");
    fs::write(&ifml_path, POLICY_SHIM_IFML).expect("write policy shim ifml");

    codegraph::driver::run(codegraph::driver::RunArgs {
        schemas: None,
        classifier: None,
        config_path: &fixture.config,
        output: &output,
        extension_points_path: None,
        integration_manifests: &[],
        profile_name: "default",
        variant: None,
        profiles_config_path: Some(profiles_path),
        no_post_gen: true,
        template_dir: &[],
        ifml_files: std::slice::from_ref(&ifml_path),
        openapi_files: &[],
        mox_files: std::slice::from_ref(&fixture.mox),
        rosetta_files: &[],
        ddd_files: std::slice::from_ref(&fixture.ddd),
        evt_files: &[],
        deploy_files: &[],
        ifml_framework: &[],
        ifml_components: None,
        ifml_design_system: None,
        codegraph_rev: None,
        check: false,
        ux_rules: None,
    })
    .await
    .expect("generation over the service-ops fixture must succeed");

    // 2. The emitted surface must be complete BEFORE any build spend.
    assert_emitted_surface(&output).expect("the emitted actions contract must carry every leg");
    let actions = fs::read_to_string(actions_path(&output)).expect("read actions contract");
    println!("emitted actions contract:\n{actions}");

    // 3. Scratch DB: create → prelude → migrations → API key.
    let db = GateDb::create(base).await.expect("scratch database");
    db.apply_prelude().await.expect("prelude");
    let db_url = db.url();
    let scratch = codegraph_ops::pg::PgTarget::from_url(&db_url).expect("scratch target");
    let (migrations_dir, _staged) = resolve_migrations_dir(&scratch, &output).await;
    db.apply_migrations(&migrations_dir)
        .await
        .expect("migrations");
    let api_key = db.provision_api_key().await.expect("api key");

    let result = async {
        // 4. Build + boot the generated app (legacy pool mode is fine).
        let bin = axum_server::build_app(
            &output.join("Cargo.toml"),
            &output,
            &gate_target_dir(),
            &logs_dir(),
            "app",
        )?;
        let jwt_secret = "ws1-service-ops-gate-jwt-secret-0123456789abcdef";
        let server = axum_server::boot_server(
            &bin,
            &[
                ("DATABASE_URL", db_url.as_str()),
                ("SUPABASE_JWT_SECRET", jwt_secret),
                ("APP_NAME", "service-ops-gate-app"),
            ],
            "service-ops-gate-app",
            &logs_dir(),
        )
        .await?;
        println!("gate: server booted on 127.0.0.1:{}", server.port);

        // 5. hurl: the EMITTED state-aware contract (own row: create 201 →
        //    inspect/archive 200 → reverse 409 → submit 200 → approve 200 →
        //    notify 501 → approve replay 409 → purge 200) then the
        //    hand-authored wrong-state probe on its own row.
        let (create_url, create_body) =
            extract_create_entry(&actions).map_err(|e| format!("CREATE entry: {e}"))?;
        let (item_base, capture) =
            extract_action_base(&actions).map_err(|e| format!("action base: {e}"))?;
        let probe_body = stamp_probe_row(&create_body)?;
        let capture_name = capture
            .trim_start_matches("{{")
            .trim_end_matches("}}")
            .to_string();
        let probe = wrong_state_probe(
            &create_url,
            &item_base,
            &capture,
            &capture_name,
            &probe_body,
        );
        let probe_path = gate_root().join("wrong_state_probe.hurl");
        fs::write(&probe_path, probe).map_err(|e| format!("write probe file: {e}"))?;
        run_hurl(server.port, &api_key, &[actions_path(&output), probe_path])?;

        // 6. DB-level proof: the probe row really advanced to Calculating
        //    (the machine state column stores the enum code directly).
        let status = codegraph_ops::db::psql_query(
            &scratch,
            "SELECT pay_run_status FROM payroll.pay_run \
             WHERE pay_run_id = 'ws1-gate-pay-run';",
        )
        .await
        .map_err(|e| format!("probe status query failed: {e}"))?;
        assert_eq!(
            status.trim(),
            "Approved",
            "the probe row must sit in the Approved state over real HTTP \
             (create → submit → approve walked the machine; the 403/409 \
             denials in between must have left it untouched)"
        );
        println!("gate: probe row state over psql = {}", status.trim());
        Ok::<(), String>(())
    }
    .await;

    db.cleanup().await;
    result.unwrap();
}
