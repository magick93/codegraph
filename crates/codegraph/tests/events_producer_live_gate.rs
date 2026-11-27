//! Live events-producer publishing gate (issue #820 slice b).
//!
//! Proves — over real HTTP against the generated app on a scratch Postgres
//! with a functional pgmq stand-in — that a service-op `emits` binding
//! publishes INSIDE the command's transaction:
//!
//! 1. a machine transition publishes the planned events: the envelope lands
//!    in `pgmq.q_events_payroll` with the typed payload (record fields,
//!    previous/next state, the op clock, correlation + organization ids),
//!    the producer identity (`entity_table`/`entity_id`), and the inbound
//!    `traceparent`;
//! 2. a FAILING second publish rolls the business write back: with the
//!    audit queue dropped the action answers 500, the row stays in its
//!    initial state, and the payroll queue gains no message; restoring the
//!    queue makes the same action publish BOTH events and persist the row.
//!
//! Own fixture (the review fixture is shared). Skips gracefully without
//! hurl or Postgres.
//!
//! ```text
//! cargo test -p codegraph --test events_producer_live_gate -- --ignored --nocapture
//! ```

#[path = "gate_reset.rs"]
mod gate_reset;

#[path = "test_framework/mod.rs"]
mod test_framework;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use test_framework::axum_server;
use test_framework::pgmq::PGMQ_FUNCTIONAL_SHIM;
use test_framework::postgres::{GateDb, database_reachable, resolve_base_target};
use test_framework::process::have_tool;

const PAYROLL_MOX: &str = r#"
package nz.example.payroll

type Instant wraps String {
    format "date-time"
}

enum PayRunStatus {
    Draft as "Draft" = 0
    Posted as "Posted" = 1
}

class PayRunType {
    int total
    PayRunStatus payRunStatus
}
"#;

const PAYROLL_DDD: &str = r#"import "payroll.mox";

application Payroll {
    base nz.example.payroll

    module payroll {
        entity PayRunType scaffold repository PayRunRepository {
            findById;
            findAll;
            save;
            delete;
        }

        machine payRunStatus: PayRunStatus {
            initial Draft;
            Draft -> Posted via Approve;
            terminal Posted;
        }

        service PayRunService {
            inject PayRunRepository;
            approve => PayRunRepository.save emits PayRunPosted, PayRunAudited transition Approve;
        }
    }
}
"#;

const PAYROLL_EVT: &str = r#"import "payroll.mox"

event PayRunPosted version "1.0.0" {
    payRunId: String;
    total: int;
    payRunStatus: PayRunStatus;
    previousState: PayRunStatus;
    nextState: PayRunStatus;
    occurredOn: Instant;
    correlationId: String;
    organizationId: String;
}

event PayRunAudited version "1.0.0" {
    total: int;
}

channel payroll {
    publishes PayRunPosted;
}

channel audit {
    publishes PayRunAudited;
}
"#;

const DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.payroll]
label = "Payroll"
schema_dir = "payroll"
postgres_schema = "payroll"
"#;

const CORRELATION_ID: &str = "11111111-1111-1111-1111-111111111111";
const TRACEPARENT: &str = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";

fn gate_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/events-producer-gate")
}

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

/// The workspace default profile with path deps, gRPC stripped, and the
/// producer flag ON (the same recipe as the compile gate).
fn write_profiles(dir: &Path) -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    let type_contracts = workspace_root.join("crates/codegraph-type-contracts");
    let mut text = fs::read_to_string(workspace_root.join("profiles.toml")).unwrap();
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
    text = text.replace("events_producer = false", "events_producer = true");
    let body: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim_start().starts_with("\"grpc_"))
        .collect();
    let path = dir.join("profiles.toml");
    fs::write(&path, body.join("\n")).unwrap();
    path
}

fn run_hurl(port: u16, api_key: &str, files: &[PathBuf]) -> Result<(), String> {
    let out = Command::new("hurl")
        .arg("--test")
        .arg("--error-format")
        .arg("long")
        .arg("--variable")
        .arg(format!("base_url=http://127.0.0.1:{port}"))
        .arg("--variable")
        .arg(format!("api_key={api_key}"))
        .args(files)
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

/// Install pgmq the way a project's own `0003_pgmq_setup.sql` does.
///
/// The Supabase image pre-creates the `pgmq` schema as its superuser (the
/// #727 M6 prelude constraint), and `CREATE EXTENSION` requires ownership of
/// the schema its control file declares — so the migration role (plain
/// `postgres`, NOSUPERUSER) cannot install the extension there. The generated
/// `0003_pgmq_setup.sql` and its late-binding grants then fail as tolerated
/// phase-1 warnings, leaving `app_user` with no `USAGE` on `pgmq`: every
/// producer publish answers `permission denied for schema pgmq` (42501) and
/// rolls the action back. Drop the placeholder as the admin role and let the
/// migration role create the extension itself — the ownership a real project
/// ends up with when its own migration band runs first.
///
/// Plain Postgres has no pgmq at all; there the shared functional stand-in
/// supplies the functions and this is a no-op.
async fn prepare_pgmq(db: &GateDb, target: &codegraph_ops::pg::PgTarget) -> Result<(), String> {
    let installed = scalar(
        target,
        "SELECT count(*) FROM pg_extension WHERE extname = 'pgmq';",
    )
    .await?;
    if installed == "1" {
        return Ok(());
    }
    let available = scalar(
        target,
        "SELECT count(*) FROM pg_available_extensions WHERE name = 'pgmq';",
    )
    .await?;
    if available != "1" {
        return Ok(());
    }
    db.exec_as_admin("DROP SCHEMA IF EXISTS pgmq CASCADE;")
        .await?;
    codegraph_ops::db::psql_exec(target, "CREATE EXTENSION IF NOT EXISTS pgmq;")
        .await
        .map_err(|e| e.to_string())
}

/// Stage the migration set the scratch database can apply.
///
/// With the real extension installed the generated migrations run unchanged.
/// Without it (plain Postgres) the shared functional stand-in supplies the
/// pgmq functions — applied before the band — and the generated
/// `0003_pgmq_setup.sql` loses only its `CREATE EXTENSION` line: the queue
/// creation, the grants and the trigger function still run for real.
async fn stage_migrations(
    target: &codegraph_ops::pg::PgTarget,
    app_dir: &Path,
) -> Result<(PathBuf, Option<tempfile::TempDir>, bool), String> {
    let migrations = app_dir.join("migrations");
    let installed = codegraph_ops::db::psql_query(
        target,
        "SELECT count(*) FROM pg_extension WHERE extname = 'pgmq';",
    )
    .await
    .map_err(|e| e.to_string())?
        == "1";
    if installed {
        return Ok((migrations, None, false));
    }
    let staged = tempfile::tempdir().map_err(|e| e.to_string())?;
    let dst = staged.path().join("migrations");
    fs::create_dir_all(&dst).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(&migrations).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let dst_path = dst.join(&name);
        if name.to_string_lossy().contains("pgmq_setup") {
            let generated = fs::read_to_string(entry.path()).map_err(|e| e.to_string())?;
            let shimmed = generated.replace(
                "CREATE EXTENSION IF NOT EXISTS pgmq;",
                "-- pgmq is supplied by the functional stand-in",
            );
            fs::write(&dst_path, shimmed).map_err(|e| e.to_string())?;
        } else {
            fs::copy(entry.path(), &dst_path).map_err(|e| e.to_string())?;
        }
    }
    Ok((dst, Some(staged), true))
}

const PROBE_PUBLISH: &str = r#"# Events-producer probe: create a Draft row and approve it.
POST {{base_url}}/api/v1/payroll/pay-run
Authorization: Bearer {{api_key}}
{
  "total": 42,
  "pay_run_status": "Draft"
}

HTTP 201
[Captures]
row_id: jsonpath "$.data.id"

POST {{base_url}}/api/v1/payroll/pay-run/{{row_id}}/actions/approve
Authorization: Bearer {{api_key}}
X-Correlation-ID: 11111111-1111-1111-1111-111111111111
traceparent: 00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01

HTTP 200
"#;

const PROBE_ROLLBACK_FAIL: &str = r#"# With the audit queue dropped the second publish fails; the whole action
# rolls back (500).
POST {{base_url}}/api/v1/payroll/pay-run
Authorization: Bearer {{api_key}}
{
  "total": 43,
  "pay_run_status": "Draft"
}

HTTP 201
[Captures]
row_id: jsonpath "$.data.id"

POST {{base_url}}/api/v1/payroll/pay-run/{{row_id}}/actions/approve
Authorization: Bearer {{api_key}}

HTTP 500
"#;

const PROBE_ROLLBACK_SUCCEED: &str = r#"# After the audit queue is recreated the same action publishes both events.
POST {{base_url}}/api/v1/payroll/pay-run/@ROW_ID@/actions/approve
Authorization: Bearer {{api_key}}

HTTP 200
"#;

async fn scalar(target: &codegraph_ops::pg::PgTarget, sql: &str) -> Result<String, String> {
    codegraph_ops::db::psql_query(target, sql)
        .await
        .map(|value| value.trim().to_string())
        .map_err(|e| e.to_string())
}

#[tokio::test]
#[ignore = "live gate: needs Postgres, hurl, and a cargo build of the generated app"]
async fn producer_events_publish_in_the_command_transaction() {
    if let Err(reason) = gate_prerequisites() {
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

    // 1. Generate the publishing fixture app.
    let fixture = tempfile::tempdir().expect("fixture tempdir");
    let root = fixture.path();
    fs::write(root.join("payroll.mox"), PAYROLL_MOX).unwrap();
    fs::write(root.join("payroll.ddd"), PAYROLL_DDD).unwrap();
    fs::write(root.join("payroll.evt"), PAYROLL_EVT).unwrap();
    fs::write(root.join("domains.toml"), DOMAINS_TOML).unwrap();
    let profiles = write_profiles(root);
    let output = gate_app_dir();
    gate_reset::reset_generated_source(&output).expect("reset the gate app dir");
    fs::create_dir_all(&output).unwrap();
    codegraph::driver::run(codegraph::driver::RunArgs {
        schemas: None,
        classifier: None,
        config_path: &root.join("domains.toml"),
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
        mox_files: &[root.join("payroll.mox")],
        rosetta_files: &[],
        ddd_files: &[root.join("payroll.ddd")],
        evt_files: &[root.join("payroll.evt")],
        deploy_files: &[],
        ifml_framework: &[],
        ifml_components: None,
        ifml_design_system: None,
        codegraph_rev: None,
        check: false,
        ux_rules: None,
    })
    .await
    .expect("generation over the publishing fixture must succeed");

    // 2. Scratch DB: prelude → pgmq → migrations → API key.
    let db = GateDb::create(base).await.expect("scratch database");
    db.apply_prelude().await.expect("prelude");
    let db_url = db.url();
    let scratch = codegraph_ops::pg::PgTarget::from_url(&db_url).expect("scratch target");
    prepare_pgmq(&db, &scratch)
        .await
        .expect("prepare pgmq for the migration role");
    let (migrations_dir, _staged, needs_shim) = stage_migrations(&scratch, &output)
        .await
        .expect("stage migrations");
    if needs_shim {
        codegraph_ops::db::psql_exec(&scratch, PGMQ_FUNCTIONAL_SHIM)
            .await
            .expect("apply the functional pgmq stand-in");
    }
    db.apply_migrations(&migrations_dir)
        .await
        .expect("migrations");
    let api_key = db.provision_api_key().await.expect("api key");
    assert_eq!(
        scalar(
            &scratch,
            "SELECT has_schema_privilege('app_user','pgmq','USAGE')::text;",
        )
        .await
        .expect("pgmq usage probe"),
        "true",
        "the migration band must grant app_user USAGE on pgmq"
    );

    let result = async {
        // 3. Build + boot.
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
                    "ws820-producer-gate-secret-0123456789",
                ),
                ("APP_NAME", "events-producer-gate-app"),
            ],
            "events-producer-gate-app",
            &logs_dir(),
        )
        .await?;
        println!("gate: server booted on 127.0.0.1:{}", server.port);

        // 4. Publish leg.
        let probe = gate_root().join("probe_publish.hurl");
        fs::write(&probe, PROBE_PUBLISH).map_err(|e| format!("write probe: {e}"))?;
        run_hurl(server.port, &api_key, &[probe])?;

        let row_id = scalar(
            &scratch,
            "SELECT id FROM payroll.pay_run ORDER BY created_at LIMIT 1;",
        )
        .await?;
        let tenant = scalar(
            &scratch,
            "SELECT platform_organization_id FROM payroll.pay_run ORDER BY created_at LIMIT 1;",
        )
        .await?;
        let count = scalar(&scratch, "SELECT count(*) FROM pgmq.q_events_payroll;").await?;
        assert_eq!(count, "1", "exactly one PayRunPosted message");
        assert_eq!(
            scalar(
                &scratch,
                "SELECT message->>'event' FROM pgmq.q_events_payroll;"
            )
            .await?,
            "PayRunPosted"
        );
        assert_eq!(
            scalar(
                &scratch,
                "SELECT message->'payload'->>'pay_run_id' FROM pgmq.q_events_payroll;"
            )
            .await?,
            row_id,
            "the surrogate id binds the entity id payload field"
        );
        assert_eq!(
            scalar(
                &scratch,
                "SELECT message->'payload'->>'pay_run_status' FROM pgmq.q_events_payroll;"
            )
            .await?,
            "Posted"
        );
        assert_eq!(
            scalar(
                &scratch,
                "SELECT message->'payload'->>'previous_state' FROM pgmq.q_events_payroll;"
            )
            .await?,
            "Draft"
        );
        assert_eq!(
            scalar(
                &scratch,
                "SELECT message->'payload'->>'next_state' FROM pgmq.q_events_payroll;"
            )
            .await?,
            "Posted"
        );
        assert_eq!(
            scalar(
                &scratch,
                "SELECT message->'payload'->>'total' FROM pgmq.q_events_payroll;"
            )
            .await?,
            "42"
        );
        assert_eq!(
            scalar(
                &scratch,
                "SELECT message->'payload'->>'correlation_id' FROM pgmq.q_events_payroll;"
            )
            .await?,
            CORRELATION_ID
        );
        assert_eq!(
            scalar(
                &scratch,
                "SELECT message->'payload'->>'organization_id' FROM pgmq.q_events_payroll;"
            )
            .await?,
            tenant
        );
        assert!(
            scalar(
                &scratch,
                "SELECT message->'payload'->>'occurred_on' FROM pgmq.q_events_payroll;"
            )
            .await?
            .starts_with("20"),
            "the op clock is an ISO timestamp"
        );
        assert_eq!(
            scalar(
                &scratch,
                "SELECT message->>'entity_table' FROM pgmq.q_events_payroll;"
            )
            .await?,
            "pay_run"
        );
        assert_eq!(
            scalar(
                &scratch,
                "SELECT message->>'entity_id' FROM pgmq.q_events_payroll;"
            )
            .await?,
            row_id
        );
        assert_eq!(
            scalar(
                &scratch,
                "SELECT message->>'traceparent' FROM pgmq.q_events_payroll;"
            )
            .await?,
            TRACEPARENT
        );
        assert_eq!(
            scalar(
                &scratch,
                "SELECT message->>'tenant_id' FROM pgmq.q_events_payroll;"
            )
            .await?,
            tenant
        );
        println!("gate: publish proof — typed envelope in pgmq.q_events_payroll");

        // 5. Rollback leg: take the audit queue away, the action's second
        //    publish fails, and the transaction (write + first publish)
        //    rolls back. pgmq 1.5.1 registers every queue table in the
        //    extension (`ALTER EXTENSION pgmq ADD TABLE`), so `DROP TABLE`
        //    is refused; renaming the queue's table makes `pgmq.send` fail
        //    with `relation ... does not exist` — the same 42P01 a missing
        //    queue produces (a 500, not the 403 a revoked privilege maps to
        //    through the SQLSTATE classifier).
        scalar(
            &scratch,
            "ALTER TABLE pgmq.q_events_audit RENAME TO q_events_audit_unavailable;",
        )
        .await?;
        let fail_probe = gate_root().join("probe_rollback_fail.hurl");
        fs::write(&fail_probe, PROBE_ROLLBACK_FAIL).map_err(|e| format!("write probe: {e}"))?;
        run_hurl(server.port, &api_key, &[fail_probe])?;
        assert_eq!(
            scalar(&scratch, "SELECT count(*) FROM pgmq.q_events_payroll;").await?,
            "1",
            "the failed second publish must roll the first message back"
        );
        assert_eq!(
            scalar(
                &scratch,
                "SELECT pay_run_status FROM payroll.pay_run ORDER BY created_at DESC LIMIT 1;"
            )
            .await?,
            "Draft",
            "the failed publish must roll the business write back"
        );
        println!("gate: rollback proof — no message, row still Draft");

        // 6. Restore the audit queue and replay the action: both events
        //    publish and the row persists.
        scalar(
            &scratch,
            "ALTER TABLE pgmq.q_events_audit_unavailable RENAME TO q_events_audit;",
        )
        .await?;
        assert_eq!(
            scalar(&scratch, "SELECT count(*) FROM pgmq.q_events_audit;").await?,
            "1",
            "the rolled-back approve left the audit queue untouched"
        );
        let rollback_row = scalar(
            &scratch,
            "SELECT id FROM payroll.pay_run ORDER BY created_at DESC LIMIT 1;",
        )
        .await?;
        let ok_probe = gate_root().join("probe_rollback_succeed.hurl");
        fs::write(
            &ok_probe,
            PROBE_ROLLBACK_SUCCEED.replace("@ROW_ID@", rollback_row.trim()),
        )
        .map_err(|e| format!("write probe: {e}"))?;
        run_hurl(server.port, &api_key, &[ok_probe])?;
        assert_eq!(
            scalar(&scratch, "SELECT count(*) FROM pgmq.q_events_payroll;").await?,
            "2"
        );
        assert_eq!(
            scalar(&scratch, "SELECT count(*) FROM pgmq.q_events_audit;").await?,
            "2",
            "the replay publishes the audit event too (one per approve)"
        );
        assert_eq!(
            scalar(
                &scratch,
                "SELECT pay_run_status FROM payroll.pay_run ORDER BY created_at DESC LIMIT 1;"
            )
            .await?,
            "Posted"
        );
        println!("gate: replay proof — both channels published, row Posted");
        Ok::<(), String>(())
    }
    .await;

    db.cleanup().await;
    if let Err(error) = result {
        panic!("events-producer live gate failed: {error}");
    }
}

fn gate_prerequisites() -> Result<(), String> {
    if !have_tool("hurl") {
        return Err(
            "hurl is not installed — the producer gate runs the emitted app over HTTP".into(),
        );
    }
    Ok(())
}
