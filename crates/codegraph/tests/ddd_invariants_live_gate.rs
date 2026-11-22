//! Live design-invariant enforcement gate (issue #666 slices 4d+4e).
//!
//! Proves — over real HTTP against the generated app on a scratch
//! Postgres — that a modeled `invariant <expr>;` ENFORCES:
//!
//! 1. a violating CREATE answers **409** naming `INVARIANT_VIOLATION` and
//!    is COMPENSATED — no violating row survives (psql proof: the count of
//!    live violating rows is zero);
//! 2. an UPDATE to a violating state answers **409** pre-write and the row
//!    is UNCHANGED (psql proof: the seeded total never moved);
//! 3. a TRANSITION carrying a violating patch answers **409** pre-write —
//!    the machine never advances (psql proof: the state still sits at the
//!    initial value before the clean submit moves it);
//! 4. a member invariant denies the ROOT save with the `aggregateRoot`
//!    detail (the member row is stamped violating DIRECTLY in the DB —
//!    no API sequence can persist a violating member, because the member's
//!    own plane enforces the same predicate — proving the root-save member
//!    validation against a stale row).
//!
//! Own fixture (`service_ops_fixture::write_invariant_fixture` — the
//! review fixture is shared with `review_api_suite` and must not move).
//! Self-contained: skips gracefully without hurl or Postgres (the
//! `rules_guard_api_gate` precedent).
//!
//! ```text
//! cargo test -p codegraph --test ddd_invariants_live_gate -- --ignored --nocapture
//! ```

#[path = "service_ops_fixture.rs"]
mod fixture;

// The #694 source reset (shared with the other persistent-dir gates), plus its
// node-free pins.
#[path = "gate_reset.rs"]
mod gate_reset;

#[path = "test_framework/mod.rs"]
mod test_framework;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use test_framework::axum_server;
use test_framework::postgres::{GateDb, database_reachable, resolve_base_target};
use test_framework::process::have_tool;

fn gate_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/ddd-invariants-gate")
}

/// The generated app lives in the gate root (NOT a tempdir) so the
/// app-side cargo cache survives across runs — but the generated SOURCE
/// is deleted first (#687): a stale contract from an older generation
/// would be discovered by its suffix and hurl'd against a newer app.
fn gate_app_dir() -> PathBuf {
    gate_root().join("app")
}

fn gate_target_dir() -> PathBuf {
    gate_root().join("target")
}

fn logs_dir() -> PathBuf {
    gate_root().join("logs")
}

fn skip(msg: &str) {
    println!("skipping: {msg}");
}

/// hurl runs the contracts over real HTTP.
fn gate_prerequisites(hurl_available: bool) -> Result<(), String> {
    if !hurl_available {
        return Err("hurl is not installed — the invariants gate runs the \
                    emitted contracts over real HTTP"
            .to_string());
    }
    Ok(())
}

/// The fixture profiles: the workspace default with path deps (network-
/// free) and the gRPC generators stripped. Same recipe as
/// `service_ops_compile_gate` (including `type_contracts_base` — the WS1
/// finding: without it the generated domain-types crate pins this
/// checkout's unpushed rev and the build fetch fails).
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
    // The enforcement plane's feature (the #338 opt-in; the workspace
    // profile declares it false — a duplicate key is a parse error).
    text = text.replace("rosetta_backend = false", "rosetta_backend = true");
    let filtered: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim_start().starts_with("\"grpc_"))
        .collect();
    // The `rules` domain generator rides the [api] list (the #338
    // opt-in: the enforcement plane lives in the rules module).
    let body = filtered.join("\n").replace(
        "    \"errors\", \"router\", \"links\",",
        "    \"errors\", \"router\", \"links\", \"rules\",",
    );
    let path = dir.join("profiles.toml");
    fs::write(&path, body).unwrap();
    path
}

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
/// `authz_contract` precedent): staged over the generated pgmq setup
/// migration when the cluster lacks the extension.
const PGMQ_SHIM: &str = "\
-- Test shim: pgmq stand-in (no extension available).
CREATE SCHEMA IF NOT EXISTS pgmq;
CREATE OR REPLACE FUNCTION pgmq.create(p_name text) RETURNS void
AS $shim$ BEGIN END $shim$ LANGUAGE plpgsql;
CREATE OR REPLACE FUNCTION pgmq.send(p_queue text, p_msg jsonb) RETURNS bigint
AS $shim$ SELECT 1 $shim$ LANGUAGE sql;
";

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

fn contract_path(output: &Path, suffix: &str) -> PathBuf {
    let hurl_dir = output.join("hurl");
    std::fs::read_dir(&hurl_dir)
        .ok()
        .and_then(|entries| {
            entries.filter_map(|e| e.ok()).map(|e| e.path()).find(|p| {
                p.extension().is_some_and(|x| x == "hurl")
                    && p.file_name()
                        .is_some_and(|n| n.to_string_lossy().ends_with(suffix))
            })
        })
        .unwrap_or_else(|| hurl_dir.join(suffix))
}

/// The emitted contracts must carry their invariant legs BEFORE any build
/// spend: the violating-create legs pin the 409 + the INVARIANT_VIOLATION
/// assert on both the root and the member contract.
fn assert_emitted_surface(output: &Path) -> Result<(), String> {
    for (suffix, why) in [
        ("_payroll_pay_run.hurl", "the root contract"),
        ("_payroll_pay_run_line.hurl", "the member contract"),
    ] {
        let path = contract_path(output, suffix);
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("{} is missing ({}): {e}", path.display(), why))?;
        if !content.contains("jsonpath \"$.error.code\" == \"INVARIANT_VIOLATION\"") {
            return Err(format!(
                "{why} lacks the INVARIANT_VIOLATION leg:\n{content}"
            ));
        }
        if !content.contains("HTTP 409") {
            return Err(format!("{why} lacks the 409 pin:\n{content}"));
        }
    }
    Ok(())
}

/// Probe part 1: own stamped row — violating transition patch (409,
/// machine untouched), clean transition (200), violating update (409),
/// and the member child seeded VALID (quantity = 5) so the part-2 stamp
/// has a row to corrupt.
const PROBE_PART1: &str = r#"# WS666 design-invariant probe, part 1 — hand-authored, own row.
POST {{base_url}}/api/v1/payroll/pay-run
Authorization: Bearer {{api_key}}
{
  "pay_run_id": "ws666-gate-pay-run",
  "pay_run_status": "Draft",
  "total": 42
}

HTTP 201
[Captures]
probe_id: jsonpath "$.data.id"

# Transition violation (criterion 3): the submit patch carries total = -1.
# The state move applies to the CANDIDATE, the invariant fails, and the
# 409 answers BEFORE the write — the machine never advances.
POST {{base_url}}/api/v1/payroll/pay-run/{{probe_id}}/actions/submit
Authorization: Bearer {{api_key}}
{
  "total": -1
}

HTTP 409
[Asserts]
jsonpath "$.error.code" == "INVARIANT_VIOLATION"
jsonpath "$.error.details[0].field" == "class"
jsonpath "$.error.details[0].message" == "PayRunType"
jsonpath "$.error.details[1].field" == "invariantIndex"
jsonpath "$.error.details[1].message" == "0"

# Clean transition: the invariant holds, Draft -> Calculated.
POST {{base_url}}/api/v1/payroll/pay-run/{{probe_id}}/actions/submit
Authorization: Bearer {{api_key}}

HTTP 200

# Update violation (criterion 2): the patch alone violates; the row is
# unchanged (proven over psql after the run).
PUT {{base_url}}/api/v1/payroll/pay-run/{{probe_id}}
Authorization: Bearer {{api_key}}
{
  "total": -1
}

HTTP 409
[Asserts]
jsonpath "$.error.code" == "INVARIANT_VIOLATION"

# The member child, seeded VALID — part 2 corrupts it in the DB to prove
# the root save's member validation. Its VIOLATING twin (quantity = -1)
# runs first: the nested violating-create leg answers 409 and compensates.
POST {{base_url}}/api/v1/payroll/pay-run/{{probe_id}}/pay-run-line
Authorization: Bearer {{api_key}}
{
  "line_id": "ws666-violating-line",
  "quantity": -1
}

HTTP 409
[Asserts]
jsonpath "$.error.code" == "INVARIANT_VIOLATION"
jsonpath "$.error.details[0].message" == "PayRunLineType"
jsonpath "$.error.details[1].field" == "invariantIndex"

POST {{base_url}}/api/v1/payroll/pay-run/{{probe_id}}/pay-run-line
Authorization: Bearer {{api_key}}
{
  "line_id": "ws666-gate-line",
  "quantity": 5
}

HTTP 201
"#;

/// Probe part 2 (after the psql stamp): the ROOT save with a VALID total
/// is denied because a MEMBER row violates its invariant — the 409 names
/// the member class, its index, and the aggregate root. `@PROBE_ID@` is
/// resolved from the DB between the parts (captures do not cross hurl
/// files).
const PROBE_PART2: &str = r#"# WS666 design-invariant probe, part 2 — the member invariant denies the
# root save (the member row was stamped violating directly in the DB).
PUT {{base_url}}/api/v1/payroll/pay-run/@PROBE_ID@
Authorization: Bearer {{api_key}}
{
  "total": 43
}

HTTP 409
[Asserts]
jsonpath "$.error.code" == "INVARIANT_VIOLATION"
jsonpath "$.error.details[0].message" == "PayRunLineType"
jsonpath "$.error.details[2].field" == "aggregateRoot"
jsonpath "$.error.details[2].message" == "PayRunType"
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "live gate: needs Postgres, hurl, and a cargo build of the generated app"]
async fn design_invariants_enforce_over_http() {
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

    // 1. Generate the invariant fixture app (the compile-gate recipe).
    println!("generating the design-invariant fixture app…");
    let fixture = fixture::write_invariant_fixture();
    let _engine = fixture::ingest_invariant(&fixture).await;
    let output = gate_app_dir();
    gate_reset::reset_generated_source(&output).expect("reset the gate app dir");
    let fixture_root = fixture.config.parent().expect("fixture root").to_path_buf();
    let profiles_path = write_profiles(&fixture_root);

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
        ifml_files: &[],
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
    .expect("generation over the invariant fixture must succeed");

    // 2. The emitted contracts carry their invariant legs BEFORE the
    //    build spend.
    assert_emitted_surface(&output).expect("the emitted contracts must carry the invariant legs");

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
        // 4. Build + boot the generated app.
        let bin = axum_server::build_app(
            &output.join("Cargo.toml"),
            &output,
            &gate_target_dir(),
            &logs_dir(),
            "app",
        )?;
        let server = axum_server::boot_server(
            &bin,
            &[
                ("DATABASE_URL", db_url.as_str()),
                (
                    "SUPABASE_JWT_SECRET",
                    "ws666-invariants-gate-secret-0123456789",
                ),
                ("APP_NAME", "ddd-invariants-gate-app"),
            ],
            "ddd-invariants-gate-app",
            &logs_dir(),
        )
        .await?;
        println!("gate: server booted on 127.0.0.1:{}", server.port);

        // 5. hurl the EMITTED contracts: the root contract's violating-
        //    create leg answers 409 (+ compensation) and every
        //    non-invariant leg stays green; the actions contract walks
        //    the machine. The CHILD contract is pinned by shape only
        //    (`assert_emitted_surface`) — a child contract's LIST entry
        //    reads the parent capture it introduces later, so it cannot
        //    run standalone (the ops `[hurl]` skip convention); its
        //    nested violating-create leg runs live in probe part 1.
        run_hurl(
            server.port,
            &api_key,
            &[
                contract_path(&output, "_payroll_pay_run.hurl"),
                contract_path(&output, "_payroll_pay_run_actions.hurl"),
            ],
        )?;

        // 6. The hand-authored probe, part 1: transition + update
        //    violations on the stamped row, the clean transition, and the
        //    valid member child.
        let probe1 = gate_root().join("probe_part1.hurl");
        fs::write(&probe1, PROBE_PART1).map_err(|e| format!("write probe: {e}"))?;
        run_hurl(server.port, &api_key, &[probe1])?;

        // 7. The member stamp: corrupt the child row DIRECTLY in the DB —
        //    no API sequence can persist a violating member (the member's
        //    own plane enforces the same predicate), so the root-save
        //    member validation is proven against a stale row.
        codegraph_ops::db::psql_query(
            &scratch,
            "UPDATE payroll.pay_run_line SET quantity = -1 \
             WHERE line_id = 'ws666-gate-line';",
        )
        .await
        .map_err(|e| format!("member stamp failed: {e}"))?;

        // 8. Probe part 2: the root save with a VALID total is denied by
        //    the member invariant (409 naming class + aggregateRoot). The
        //    probe row's id resolves from the DB — captures do not cross
        //    hurl files.
        let probe_row_id = codegraph_ops::db::psql_query(
            &scratch,
            "SELECT id FROM payroll.pay_run WHERE pay_run_id = 'ws666-gate-pay-run';",
        )
        .await
        .map_err(|e| format!("probe id lookup failed: {e}"))?;
        let probe2_body = PROBE_PART2.replace("@PROBE_ID@", probe_row_id.trim());
        if !probe2_body.contains("pay-run/") || probe2_body.contains("@PROBE_ID@") {
            return Err(format!(
                "probe id lookup returned nothing usable: {probe_row_id:?}"
            ));
        }
        let probe2 = gate_root().join("probe_part2.hurl");
        fs::write(&probe2, probe2_body).map_err(|e| format!("write probe: {e}"))?;
        run_hurl(server.port, &api_key, &[probe2])?;

        // 9. psql proofs.
        //    (a) Criterion 1: the violating create is COMPENSATED — no
        //    live row with a violating total (the contract's leg AND the
        //    update/transition probes' -1 never persisted).
        let violating = codegraph_ops::db::psql_query(
            &scratch,
            "SELECT count(*) FROM payroll.pay_run WHERE total < 0 AND deleted_at IS NULL;",
        )
        .await
        .map_err(|e| format!("compensation proof failed: {e}"))?;
        assert_eq!(
            violating.trim(),
            "0",
            "a violating create must be compensated (row removed); \
             found {violating} live violating rows"
        );
        println!("gate: compensation proof — 0 live violating rows");

        //    (b) Criterion 2: the denied update left the row UNCHANGED.
        let total = codegraph_ops::db::psql_query(
            &scratch,
            "SELECT total FROM payroll.pay_run WHERE pay_run_id = 'ws666-gate-pay-run';",
        )
        .await
        .map_err(|e| format!("update proof failed: {e}"))?;
        assert_eq!(
            total.trim(),
            "42",
            "the denied update must leave the row unchanged (the probe's \
             valid PUT in part 2 was member-denied and the part-1 PUT was \
             invariant-denied); found total = {}",
            total.trim()
        );
        println!("gate: update proof — total unchanged at {}", total.trim());

        //    (c) Criterion 3: the violating transition never advanced the
        //    machine (the clean submit moved it exactly once).
        let status = codegraph_ops::db::psql_query(
            &scratch,
            "SELECT pay_run_status FROM payroll.pay_run \
             WHERE pay_run_id = 'ws666-gate-pay-run';",
        )
        .await
        .map_err(|e| format!("transition proof failed: {e}"))?;
        assert_eq!(
            status.trim(),
            "Calculated",
            "the violating transition must not advance the machine; the \
             clean submit lands exactly one edge; found {}",
            status.trim()
        );
        println!("gate: transition proof — state = {}", status.trim());

        //    (d) The member stamp survives the denied root save (nothing
        //    about the member was written).
        let quantity = codegraph_ops::db::psql_query(
            &scratch,
            "SELECT quantity FROM payroll.pay_run_line WHERE line_id = 'ws666-gate-line';",
        )
        .await
        .map_err(|e| format!("member proof failed: {e}"))?;
        assert_eq!(
            quantity.trim(),
            "-1",
            "the member row must be untouched by the denied root save; \
             found quantity = {}",
            quantity.trim()
        );
        println!(
            "gate: member proof — quantity untouched at {}",
            quantity.trim()
        );
        Ok::<(), String>(())
    }
    .await;

    db.cleanup().await;
    result.unwrap();
}
